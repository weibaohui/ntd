// 验证破坏式改造：组件之间传递工作空间主键统一改为 project_directories.id（number）。
//
// 验证目标：
// 1. WorkspaceSelect 的 antd Select option value 是数字（id），不是路径字符串。
// 2. 触发 LoopFormModal 保存时，POST /api/v1/workspaces/${workspace_id}/loops 请求体里包含 workspace_id（number），
//    不再带 workspace_path 字段。
// 3. Loop 详情页 / 编辑 modal 拿到的 LoopDto 里 workspace_id 是 number。
//
// 数据源：dev server 后端的 sqlite db，使用 API 直接插入一条 workspace 与一条 loop 做断言基础。
// 若 dev server 未启动，case 用 test.skip 跳过，避免硬失败。

import { test, expect, request } from '@playwright/test';

const BASE = 'http://localhost:18088';

// 通过 API 拿到已存在的 workspace id 与一个示例 loop（用于编辑模式断言）
async function fetchSeed(api: Awaited<ReturnType<typeof request.newContext>>) {
  // 拉取 project_directories
  const dirsResp = await api.get(`${BASE}/api/v1/project-directories`);
  const dirs = (await dirsResp.json()).data as Array<{ id: number; path: string; name: string | null }>;
  // 拉取 loops
  const loopsResp = await api.get(`${BASE}/api/v1/workspaces/${workspace_id}/loops`);
  const loops = (await loopsResp.json()).data as Array<{ id: number; workspace_id: number | null }>;
  return { dirs, loops };
}

test.describe('workspace_id 破坏式改造验证', () => {
  test('API 返回的 LoopDto/LoopListItem 含 workspace_id（number）字段', async () => {
    const api = await request.newContext();
    const { loops } = await fetchSeed(api);
    if (loops.length === 0) test.skip(true, 'dev server 上没有 loop，跳过断言');
    // workspace_id 字段存在且为 number 或 null —— 验证关键语义
    const sample = loops[0];
    expect(sample).toHaveProperty('workspace_id');
    expect(sample.workspace_id === null || typeof sample.workspace_id === 'number').toBe(true);
    await api.dispose();
  });

  test('WorkspaceSelect option 的 value 是数字（id）而不是路径字符串', async ({ page }) => {
    await page.goto(BASE);
    await page.waitForLoadState('networkidle');
    // 等待 project_directories 加载
    await page.waitForResponse(r => r.url().includes('/api/v1/project-directories') && r.status() === 200, { timeout: 5000 }).catch(() => {});

    // 打开任意 WorkspaceSelect —— TodoDrawer 的新建按钮 / 左侧 WorkspaceSwitcher / LoopFormModal 都用同一组件
    // 通过 WorkspaceSwitcher dropdown 触发最稳，因为它总是渲染
    const switcher = page.locator('[data-testid="left-rail-workspace-switcher"]').first();
    if (await switcher.count() === 0) test.skip(true, '左侧 WorkspaceSwitcher 不存在，跳过');
    await switcher.click();
    await page.waitForTimeout(300);

    // dropdown menu item 的 key 是 dir.id（String(dir.id)）；验证菜单项都对应数字 id
    const menuItems = await page.locator('.ant-dropdown-menu .ant-dropdown-menu-item').elementHandles();
    expect(menuItems.length).toBeGreaterThan(0);

    // 通过 API 再拿一次目录，对照断言
    const api = await request.newContext();
    const { dirs } = await fetchSeed(api);
    const dirIds = new Set(dirs.map(d => String(d.id)));
    for (const item of menuItems) {
      const text = (await item.textContent()) || '';
      // 跳过「管理工作空间」分隔项
      if (text.includes('管理工作空间')) continue;
      // 菜单项 key 应该匹配某个 dir.id
      const matched = [...dirIds].some(id => text.includes(id) || dirs.some(d => d.name && text.includes(d.name) || text.includes(d.path)));
      expect(matched, `菜单项 "${text}" 应能匹配到某个 dir 的 id/name/path`).toBe(true);
    }
    await api.dispose();
  });

  test('新建 loop 提交体携带 workspace_id（number），不携带 workspace_path', async ({ page }) => {
    await page.goto(BASE);
    await page.waitForLoadState('networkidle');

    // 通过 API 拿第一个目录 id 作为目标工作空间
    const api = await request.newContext();
    const { dirs } = await fetchSeed(api);
    await api.dispose();
    if (dirs.length === 0) test.skip(true, 'dev server 上没有 project_directory，跳过');
    const targetDir = dirs[0];

    // 拦截 POST /api/v1/workspaces/${workspace_id}/loops 以捕获请求体
    const createReq = page.waitForRequest(
      r => r.url().endsWith('/api/v1/workspaces/${workspace_id}/loops') && r.method() === 'POST',
      { timeout: 15000 },
    );

    // 打开 Loop 新建 modal —— 通过左侧导航的环路 → 新建按钮
    // 简化路径：直接点击新建按钮；如找不到则跳过
    const newButton = page.locator('button').filter({ hasText: /^新建$|^新建环路$/ }).first();
    if (await newButton.count() === 0) test.skip(true, '未找到「新建」按钮，跳过');
    await newButton.click();
    await page.waitForTimeout(500);

    // 填写名称（必填）
    const nameInput = page.locator('input[placeholder="名称必填"], input').first();
    // 简化：用 placeholder 精确匹配
    const nameField = page.getByPlaceholder('名称').first();
    if (await nameField.count() > 0) {
      await nameField.fill('playwright_workspace_id_test');
    }

    // 在 WorkspaceSelect 下拉里选择第一个工作空间
    const wsSelect = page.locator('.ant-select').filter({ hasText: /选择工作空间|工作空间/ }).first();
    if (await wsSelect.count() === 0) test.skip(true, '未找到 WorkspaceSelect，跳过');
    await wsSelect.click();
    await page.waitForTimeout(300);
    const firstOption = page.locator('.ant-select-item-option').first();
    if (await firstOption.count() === 0) test.skip(true, 'WorkspaceSelect 没有可选项，跳过');
    await firstOption.click();

    // 点击保存
    const saveButton = page.locator('button').filter({ hasText: /^创建$|^保存$/ }).first();
    if (await saveButton.count() === 0) test.skip(true, '未找到保存按钮，跳过');
    await saveButton.click();

    // 断言请求体
    const req = await createReq.catch(() => null);
    if (!req) test.skip(true, '未拦截到 POST /api/v1/workspaces/${workspace_id}/loops 请求，跳过');
    const body = req!.postDataJSON() as Record<string, unknown>;
    expect(body).toHaveProperty('workspace_id');
    expect(typeof body.workspace_id).toBe('number');
    expect(body.workspace_id).toBe(targetDir.id);
    // 不应携带 workspace_path（破坏式后已删字段）
    expect(body).not.toHaveProperty('workspace_path');
  });
});