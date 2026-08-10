// 094 验证：WS 广播 workspace 过滤——连接参数声明与切换重连。
//
// 验证点：
// 1. 首连 URL 携带 ?workspace_id=<初始 workspace>（localStorage 清空时为目录列表第一项）。
// 2. 切换 workspace 后旧连接关闭、新连接携带新 workspace_id。
// 3. 全程 console 无错误（重连流程无异常）。
import { test, expect } from '@playwright/test';

test('094 WS 连接携带 workspace_id 且切换 workspace 后重连换参', async ({ page }) => {
  const consoleErrors: string[] = [];
  page.on('console', msg => { if (msg.type() === 'error') consoleErrors.push(msg.text()); });
  page.on('pageerror', err => consoleErrors.push(`pageerror: ${err.message}`));

  // 清空持久化的 workspace 选择，保证初始化为目录列表第一项（id=1），测试可重复
  await page.addInitScript(() => {
    try { localStorage.removeItem('ntd_selected_workspace'); } catch {}
  });

  // 捕获所有 WS 连接（page.on('websocket') 对后续每次 new WebSocket 都触发）
  const wsUrls: string[] = [];
  page.on('websocket', ws => {
    wsUrls.push(ws.url());
    console.log('WS 连接建立:', ws.url());
  });

  await page.goto('http://localhost:18088');
  // 初始 workspace 需一次异步目录加载后才确定（useApp 初始化 dispatch SELECT_WORKSPACE），
  // WS 首连在此之前可能不带参——用 poll 等待「带参连接」出现
  await expect.poll(async () => wsUrls.some(u => u.includes('workspace_id=')), { timeout: 15000 }).toBe(true);

  const firstParam = new URL(wsUrls.find(u => u.includes('workspace_id='))!).searchParams.get('workspace_id');
  console.log('首连 workspace_id:', firstParam);
  expect(firstParam).not.toBeNull();

  // 拿目录列表确定首连 workspace 的名称（菜单未传 selectedKeys，无法用 -selected 类排除当前项，
  // 按名称排除最可靠）
  const dirs: Array<{ id: number; name: string }> = await page.evaluate(async () => {
    const resp = await fetch('/api/v1/project-directories');
    const body = await resp.json();
    return body.data;
  });
  const currentName = dirs.find(d => String(d.id) === firstParam)?.name;
  const targetName = dirs.find(d => String(d.id) !== firstParam)?.name;
  console.log('当前:', currentName, '→ 目标:', targetName);
  expect(targetName, 'dev 库应至少有两个 workspace').toBeTruthy();

  // 切换 workspace：点切换器 → 选目标名称的菜单项
  // 切换器两形态：展开态 left-rail-workspace-switcher / 折叠态 left-rail-workspace，
  // 取决于 rail 折叠状态（localStorage 持久化），哪个可见点哪个
  const switcher = page.locator('[data-testid="left-rail-workspace-switcher"], [data-testid="left-rail-workspace"]').first();
  await expect(switcher).toBeVisible({ timeout: 10000 });
  await switcher.click();
  const menu = page.locator('.ant-dropdown-menu:visible');
  await expect(menu.first()).toBeVisible();
  await menu.locator('.ant-dropdown-menu-item', { hasText: targetName }).first().click();

  // 断言：新 WS 连接建立，且 workspace_id 与首连不同
  await expect.poll(async () => {
    const params = wsUrls.map(u => new URL(u).searchParams.get('workspace_id')).filter(Boolean);
    return new Set(params).size;
  }, { timeout: 15000 }).toBeGreaterThan(1);

  const lastParam = new URL(wsUrls[wsUrls.length - 1]).searchParams.get('workspace_id');
  console.log('切换后 workspace_id:', lastParam, '全部连接:', JSON.stringify(wsUrls));
  expect(lastParam).not.toBe(firstParam);

  expect(consoleErrors, `console 不应有错误: ${consoleErrors.join('; ')}`).toEqual([]);
});
