// 014-任务删除流程 Playwright 验证（NTD-014-A + 评审补充）。
// 验证点：
// 1. 任务详情页删除按钮弹出 Popconfirm，标题「确定删除任务？」，
//    description 明确提示「将同时删除该任务的全部讨论记录（含执行回帖），此操作不可恢复。」
// 2. 点「取消」：任务保留（API 回读仍在）
// 3. 点「删除」：跳回任务列表（/#/tasks），任务从后端消失
// 4. 关联环路不受影响（删除任务不触碰环路——NTD-014-A 核心回归点）
//
// 造数策略：直接调后端 POST /tasks 创建「委派给 mobilecoder」的任务——
// ① 响应字段是 data.task_id / data.execution_id（非 data.id，曾踩坑导致 goto 到 #/tasks/undefined）；
// ② mobilecoder 本机无 CLI，执行器无法 spawn，不产生真实 AI 消耗、也不会留下孤儿子进程
//    （此前用 zhanlu 踩坑：force-fail 只标记记录，已 spawn 的 zl 子进程会残留）；
// ③ cleanup 双保险：force-fail execution + 删除任务，避免残留运行中记录。

import { test, expect, type APIRequestContext } from '@playwright/test';

const BASE = 'http://localhost:18088';
const WS = 1;

// SPA 首屏含 Monaco/antd 等大块 JS，Chrome headless 冷启动 + 加载可能超过默认 30s，
// 单独放宽本 spec 的预算（含造数/API 校验/页面交互全链路）。
test.setTimeout(120_000);

/** 造测试任务：委派给 mobilecoder（无本机 CLI，执行器无法 spawn）。返回 {taskId, executionId}。 */
async function createTestTask(
  request: APIRequestContext,
): Promise<{ taskId: number; executionId: number }> {
  const res = await request.post(`${BASE}/api/v1/workspaces/${WS}/tasks`, {
    data: {
      requirement: 'Playwright 回归测试任务（NTD-014）',
      execution_mode: 'delegate',
      assignee_kind: 'executor',
      assignee_name: 'mobilecoder',
      auto_continue: false,
    },
  });
  expect(res.ok()).toBeTruthy();
  const body = await res.json();
  expect(body.code).toBe(0);
  // 注意：创建响应是 { execution_id, loop_id, task_id }，id 在 task_id 字段。
  return { taskId: body.data.task_id as number, executionId: body.data.execution_id as number };
}

/** 清理兜底：先 force-fail 执行（防运行中残留），再删任务（已删则容忍失败）。 */
async function cleanupTask(
  request: APIRequestContext,
  taskId: number,
  executionId: number,
): Promise<void> {
  await request
    .post(`${BASE}/api/v1/workspaces/${WS}/executions/${executionId}/force-fail`, { data: {} })
    .catch(() => {});
  await request.delete(`${BASE}/api/v1/workspaces/${WS}/tasks/${taskId}`).catch(() => {});
}

/** 任务是否仍存在（API 回读）。 */
async function taskExists(request: APIRequestContext, id: number): Promise<boolean> {
  const res = await request.get(`${BASE}/api/v1/workspaces/${WS}/tasks/${id}`);
  if (!res.ok()) return false;
  const body = await res.json();
  return body.code === 0 && body.data != null;
}

test('任务删除：确认文案/级联提示/取消保留/确认删除/环路不受影响', async ({ page, request }) => {
  // 记录删除前环路数量：删除任务绝不允许动环路（NTD-014-A 回归点）。
  const loopsBefore = await request.get(`${BASE}/api/v1/workspaces/${WS}/loops`);
  expect(loopsBefore.ok()).toBeTruthy();
  const loopCountBefore = ((await loopsBefore.json()).data as unknown[]).length;

  // 造数并打开任务详情页。
  const { taskId, executionId } = await createTestTask(request);
  try {
    await page.goto(`${BASE}/#/tasks/${taskId}`);
    // 详情页标题出现即视为路由就绪（若误落列表页，此选择器也匹配行文本，
    // 因此再等「返回列表」按钮兜底确认在详情页）。
    await page.waitForSelector('text=Playwright 回归测试任务', { timeout: 30000 });
    await page.waitForSelector('text=返回列表', { timeout: 15000 });

    // —— 1. 删除按钮 → Popconfirm 标题 + 级联提示 ——
    // 头部删除按钮含图标，accessible name 为「delete 删除」——不能用 exact 全等匹配，
    // 用默认子串匹配；讨论区帖子的删除按钮名为「delete」（无「删除」子串），不会误中。
    await page.getByRole('button', { name: '删除' }).click();
    const popconfirm = page.locator('.ant-popover').filter({ hasText: '确定删除任务？' });
    await expect(popconfirm.getByText('确定删除任务？')).toBeVisible();
    // 评审补充：明确提示级联删除讨论记录（task_posts ON DELETE CASCADE）。
    await expect(
      popconfirm.getByText(/将同时删除该任务的全部讨论记录（含执行回帖），此操作不可恢复。/),
    ).toBeVisible();

    // —— 2. 取消：任务保留 ——
    // antd 双字按钮渲染为「取 消」/「删 除」（字间空格），用忽略空格的正则匹配。
    await popconfirm.getByRole('button', { name: /取\s*消/ }).click();
    await page.waitForTimeout(600);
    expect(await taskExists(request, taskId)).toBe(true);

    // —— 3. 确认删除：跳回列表 + 任务消失 ——
    // 弹层关闭后再点头部触发按钮（此时页面仅头部按钮含「删除」子串，无歧义）。
    await page.getByRole('button', { name: '删除' }).click();
    await page
      .locator('.ant-popover')
      .filter({ hasText: '确定删除任务？' })
      .getByRole('button', { name: /删\s*除/ })
      .click();
    await page.waitForURL(/#\/tasks$/);
    expect(await taskExists(request, taskId)).toBe(false);

    // —— 4. 环路不受影响 ——
    const loopsAfter = await request.get(`${BASE}/api/v1/workspaces/${WS}/loops`);
    const loopCountAfter = ((await loopsAfter.json()).data as unknown[]).length;
    expect(loopCountAfter).toBe(loopCountBefore);
  } finally {
    // 兜底清理（正常路径任务已被删除，此处为 no-op）。
    await cleanupTask(request, taskId, executionId);
  }
});
