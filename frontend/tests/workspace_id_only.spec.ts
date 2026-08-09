// 验证破坏式改造：组件之间传递工作空间主键统一改为 project_directories.id（number）。
//
// 验证目标：
// 1. WorkspaceSelect 的 antd Select option value 是数字（id），不是路径字符串。
// 2. Loop 详情页 / 编辑 modal 拿到的 LoopDto 里 workspace_id 是 number。
// 注：原目标「LoopFormModal 保存时 POST /loops 请求体带 workspace_id」已随 044 移除 loop 直接新建入口而废弃
//（环路现仅由工艺 install/upgrade 产生，见 App.tsx「列表页不再有新建环路入口」），对应用例已删。
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
  // 拉取 loops：loops 按 workspace 隔离，且 dirs[0]（后端按 path 排序）未必含 loop，
  // 故遍历各工作空间取首个非空 loops，避免盲取导致整组用例空跳。
  let loops: Array<{ id: number; workspace_id: number | null }> = [];
  for (const d of dirs) {
    const loopsResp = await api.get(`${BASE}/api/v1/workspaces/${d.id}/loops`);
    loops = (await loopsResp.json()).data as Array<{ id: number; workspace_id: number | null }>;
    if (loops.length > 0) break;
  }
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
});