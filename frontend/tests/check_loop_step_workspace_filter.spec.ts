// 验证：Loop Studio 新增环节 Modal 中「关联环节」下拉按 loop 工作空间过滤 todo。
//
// 设计要点：
// - 找一个 workspace=/tmp/xxx 且 step_count=0 的 loop（让「添加环节」按钮可点）
// - 打开 Modal 后，从 DOM 收集 Select 下拉里的所有 option（label 形如 `#<id> <title>`）
// - 通过 /api/v1/workspaces/${workspaceId}/todos?workspace_id=<id> 拿到该工作空间下应该出现的 todo id 集合
// - 断言：下拉里的 id 全部命中目标集合（防止误把别的工作空间下的 todo 串进来）
// - 对照：全量 /api/v1/workspaces/${workspaceId}/todos 的 id 集合应该 ⊋ 目标集合（说明数据库里确实有跨空间的 todo，
//   否则用例等于没在验证过滤）
// - 兜底：若找不到可测的 loop，自动 test.skip，避免假阳性

import { test, expect, request } from '@playwright/test';

const BACKEND_URL = process.env.E2E_BACKEND_URL || 'http://localhost:18088';
const DEV_URL = 'http://localhost:18088';

// 收集 antd Select 下拉中所有 option 的 id；label 形如 "#27 讲个笑话"
async function collectOptionIds(page: import('@playwright/test').Page): Promise<number[]> {
  return await page.evaluate(() => {
    // 关联环节 Select 渲染在 Modal 内，取可见的 .ant-select-item（含 dropdown 里所有 option）
    const nodes = Array.from(document.querySelectorAll('.ant-select-item-option'));
    const ids = new Set<number>();
    for (const n of nodes) {
      // 优先用 antd 给的 value（react state 写入 DOM 的 data-* 属性缺失则用 label 兜底）
      const labelText = (n.textContent || '').trim();
      const m = labelText.match(/^#(\d+)/);
      if (m) ids.add(Number(m[1]));
    }
    return Array.from(ids);
  });
}

test('新增环节 Modal 的关联环节下拉按 loop 工作空间过滤', async () => {
  const apiCtx = await request.newContext({ baseURL: BACKEND_URL });

  // 1) 找一个带 workspace_path 且 step_count=0 的 loop，便于点「添加环节」进入新建流程
  const loopsRes = await apiCtx.get('/api/v1/workspaces/${workspaceId}/loops?page=1&limit=50');
  const loopsJson = await loopsRes.json();
  const loops: Array<{ id: number; workspace_path: string | null; step_count: number }> = loopsJson?.data ?? [];
  // 优先 /tmp/xxx（id=1），实测数据里该工作空间下确实有 todo
  const target =
    loops.find((l) => l.workspace_path === '/tmp/xxx' && l.step_count === 0) ??
    loops.find((l) => !!l.workspace_path && l.step_count === 0);
  test.skip(!target || !target.workspace_path, '没有可用的「带工作空间且无 step」的 loop，跳过用例');
  const loopId = target!.id;
  const workspacePath = target!.workspace_path!;

  // 2) 查后端确认目标 workspace 的 todo 集合，以及全量 todo 集合
  const dirsRes = await apiCtx.get('/api/v1/project-directories');
  const dirsJson = await dirsRes.json();
  const dirs: Array<{ id: number; path: string }> = dirsJson?.data ?? [];
  const dir = dirs.find((d) => d.path === workspacePath);
  test.skip(!dir, `未找到路径 ${workspacePath} 对应的工作空间目录，跳过用例`);
  const workspaceId = dir!.id;

  const targetListRes = await apiCtx.get(`/api/v1/workspaces/${workspaceId}/todos`);
  const targetListJson = await targetListRes.json();
  const targetTodos: Array<{ id: number; title: string }> = targetListJson?.data ?? [];
  const targetIds = new Set(targetTodos.map((t) => t.id));

  const allRes = await apiCtx.get('/api/v1/workspaces/${workspaceId}/todos');
  const allJson = await allRes.json();
  const allTodos: Array<{ id: number }> = allJson?.data ?? [];
  const allIds = new Set(allTodos.map((t) => t.id));

  // 若目标工作空间下根本没有 todo，过滤下拉就是空，没法证伪；同样跳过
  test.skip(targetIds.size === 0, `工作空间 ${workspacePath} 下暂无 todo，无法验证过滤`);
  // 全量必须严格多于目标集合，否则等于没在测过滤
  test.skip(allIds.size <= targetIds.size, '全量 todo 数不严格多于目标工作空间，跳过用例');

  // 3) 进入 loop 详情，打开「添加环节」
  const browser = await (await import('@playwright/test')).chromium.launch();
  const ctx = await browser.newContext({ baseURL: DEV_URL });
  const page = await ctx.newPage();
  await page.goto(`${DEV_URL}/?loop=${loopId}`);
  await page.waitForLoadState('networkidle');
  await page.waitForTimeout(1200);

  // 触发添加环节：点击流程图占位卡片（无 step 时显示「暂无执行环节，点击添加」）
  const addTrigger = page.getByText('添加环节').first();
  await expect(addTrigger, '流程图占位的「添加环节」应可见').toBeVisible({ timeout: 8000 });
  await addTrigger.click();
  await page.waitForTimeout(400);

  const modalTitle = page.getByText('新增环节').first();
  await expect(modalTitle, '新增环节 Modal 应打开').toBeVisible({ timeout: 5000 });

  // 4) 触发「关联环节」Select 的下拉展开，渲染所有 option
  // Modal 里第一个 Form.Item label 是「关联环节」
  const todoSelect = page.locator('.ant-modal .ant-select-selector').first();
  await todoSelect.click();
  await page.waitForTimeout(500);

  // 5) 收集下拉里的 id 并断言：必须全部命中目标 workspace 的 id 集合
  const optionIds = await collectOptionIds(page);
  expect(optionIds.length, '下拉至少要展示一个候选 todo').toBeGreaterThan(0);

  const foreignIds = optionIds.filter((id) => !targetIds.has(id));
  expect(
    foreignIds,
    `下拉里出现了非 ${workspacePath} 工作空间下的 todo id: ${foreignIds.join(',')}；期望只出现 id: ${Array.from(targetIds).join(',')}`,
  ).toEqual([]);

  // 同时确认：全量 todo 数 > 出现在下拉里的 todo 数 ⇒ 过滤确实起了作用
  expect(
    allIds.size,
    '全量 todo 数应严格大于下拉展示数，证明过滤生效',
  ).toBeGreaterThan(optionIds.length);

  // 6) 顺手断言 placeholder 文本透出工作空间名（让用户知道为什么只看到这些）
  const placeholder = await page
    .locator('.ant-modal .ant-select-selection-placeholder')
    .first()
    .innerText()
    .catch(() => '');
  expect(
    placeholder.includes(workspacePath),
    `placeholder 应提示「${workspacePath}」，实际: ${placeholder}`,
  ).toBe(true);

  await browser.close();
  await apiCtx.dispose();
});
