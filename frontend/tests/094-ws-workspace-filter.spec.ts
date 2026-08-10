// 094 验证：WS 广播 workspace 过滤——连接参数声明与切换重连。
//
// 验证点：
// 1. 首连 URL 携带 ?workspace_id=<初始 workspace>（localStorage 清空时为目录列表第一项）。
// 2. 切换 workspace 后旧连接关闭、新连接携带新 workspace_id。
// 3. 切换过程不产生重复连接（CodeRabbit #1011 onclose 竞态守卫的 e2e 背书）。
// 4. 全程 console 无错误（重连流程无异常）。
import { test, expect } from '@playwright/test';

test('094 WS 连接携带 workspace_id 且切换 workspace 后重连换参', async ({ page }) => {
  // console 错误收集：修复前若重连逻辑异常，通常伴随 React/WS 报错——先埋监听再操作
  const consoleErrors: string[] = [];
  page.on('console', msg => { if (msg.type() === 'error') consoleErrors.push(msg.text()); });
  page.on('pageerror', err => consoleErrors.push(`pageerror: ${err.message}`));

  // 清空持久化的 workspace 选择，保证初始化为目录列表第一项（id=1），测试可重复
  await page.addInitScript(() => {
    try { localStorage.removeItem('ntd_selected_workspace'); } catch {}
  });

  // 捕获所有 WS 连接的 URL 与关闭事件：page.on('websocket') 对每次 new WebSocket 触发；
  // close 事件记录用于「切换后旧连接确实关闭、且无同参数重复连接」的竞态断言
  const wsUrls: string[] = [];
  const closedUrls: string[] = [];
  page.on('websocket', ws => {
    wsUrls.push(ws.url());
    console.log('WS 连接建立:', ws.url());
    ws.on('close', () => {
      closedUrls.push(ws.url());
      console.log('WS 连接关闭:', ws.url());
    });
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

  // 竞态断言（CodeRabbit #1011）：等一个重连退避周期（>2s）后，
  // 不得出现第三个连接——旧 onclose 若误触发重连会产生与末次同参的重复连接。
  // 注意：Playwright 对页面主动 close() 的 close 事件捕获不可靠（实测不触发），
  // 故 close 事件仅记录观察，硬断言落在「无重复连接」上（竞态未修复时 dupCount=2）
  await page.waitForTimeout(2500);
  console.log('捕获的关闭事件:', JSON.stringify(closedUrls));
  const dupCount = wsUrls.filter(u => u === `ws://localhost:18088/api/events?workspace_id=${lastParam}`).length;
  expect(dupCount, '切换后连接不得重复（onclose 竞态回归守卫）').toBe(1);

  expect(consoleErrors, `console 不应有错误: ${consoleErrors.join('; ')}`).toEqual([]);
});
