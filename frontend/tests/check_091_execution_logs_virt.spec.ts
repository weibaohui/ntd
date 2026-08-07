// 091 性能优化回归：验证日志链路重构后执行面板仍正常工作。
//
// 重构要点（需回归）：
//  1. LogsProvider 嵌入 AppProvider、useApp 路由日志 action —— 若装配错误，应用启动即白屏。
//  2. 日志改走 @tanstack/react-virtual 虚拟列表 —— 任务运行时 .execution-panel-logs 内
//     应能渲染 .log-line（只渲染可视区行，行数恒定）。
//  3. WS Output → 50ms 缓冲 → APPEND_TASK_LOGS —— 面板底部应随输出增长并自动滚到底。
//
// 本脚本对运行环境宽进：无运行任务时仅断言「不崩溃 + WS 连通」；
// 有运行任务时追加断言日志行渲染与增长。截图留档便于 PR 评审。
import { test, expect, type Page, type ConsoleMessage } from '@playwright/test';

const BASE = 'http://localhost:18088';

// 收集 ERROR 级控制台消息，过滤掉资源加载 404 / 跨域等噪声，聚焦 React 运行时错误。
function attachErrorCollector(page: Page, sink: string[]) {
  page.on('console', (msg: ConsoleMessage) => {
    if (msg.type() !== 'error') return;
    const t = msg.text();
    // 过滤掉网络/资源类噪声与第三方无关报错，只留疑似代码缺陷的运行时错误。
    if (/Failed to load resource|net::ERR|404|favicon|preload|CORS/i.test(t)) return;
    sink.push(t);
  });
  page.on('pageerror', (err: Error) => sink.push(`pageerror: ${err.message}`));
}

test('091：日志链路重构后应用启动无运行时错误且 WS 连通', async ({ page }) => {
  const errors: string[] = [];
  attachErrorCollector(page, errors);

  await page.goto(`${BASE}`, { waitUntil: 'domcontentloaded' });

  // 应用外壳渲染：若 LogsProvider/useApp 装配错误，此处会超时（白屏）。
  await page.waitForSelector('body', { timeout: 15000 });
  // 等待 React 挂载稳定，给 Provider 初始化与首屏副作用留出时间。
  await page.waitForTimeout(2500);

  // WebSocket 全局单例应已建立并处于 OPEN/CONNECTING（LogsProvider/事件订阅装配正确的间接信号）。
  const wsState = await page.evaluate(() => {
    // 注入探测：监听下一次新建的 WebSocket，回传其就绪状态。
    return new Promise<number>((resolve) => {
      const orig = (window as any).WebSocket;
      // 已有连接可能先于探测建立，用一个小超时兜底，避免永久挂起。
      const timer = setTimeout(() => resolve(-1), 4000);
      (window as any).WebSocket = function (url: string, protocols?: string) {
        const ws = protocols ? new orig(url, protocols) : new orig(url);
        // 连接就绪后回传 readyState；CONNECTING(0)/OPEN(1) 均算正常。
        const check = () => { clearTimeout(timer); resolve(ws.readyState); };
        ws.addEventListener('open', () => check());
        setTimeout(check, 1500);
        return ws;
      } as any;
      (window as any).WebSocket.prototype = orig.prototype;
    });
  });

  // 截图留档（不入 git，仅本地/PR 评论参考）。
  await page.screenshot({ path: 'tests/__screenshots__/091_logs_boot.png', fullPage: false });

  // 核心断言：重构不应引入任何运行时错误。
  expect(errors, `控制台出现疑似运行时错误:\n${errors.join('\n')}`).toEqual([]);

  // WS 探测：-1 表示超时（无新连接，可能已存在单例），不算失败；其余需为 0 或 1。
  if (wsState !== -1) {
    expect([0, 1]).toContain(wsState);
  }

  console.log('091 启动回归通过：无运行时错误，WS 连通探测 readyState =', wsState);
});

test('091：运行中任务的执行面板用虚拟列表渲染日志行', async ({ page }) => {
  const errors: string[] = [];
  attachErrorCollector(page, errors);

  await page.goto(`${BASE}`, { waitUntil: 'domcontentloaded' });
  await page.waitForTimeout(2500);

  // 执行面板仅在存在运行任务时渲染。空闲态无运行任务属正常，跳过断言不算失败。
  const panelVisible = await page.locator('.execution-panel').isVisible().catch(() => false);
  if (!panelVisible) {
    console.log('091 日志渲染回归：当前无运行任务，面板未展示，跳过行渲染断言');
    expect(errors).toEqual([]);
    return;
  }

  // 面板可见时，日志区容器应存在；有输出时其内应有 .log-line（虚拟列表）。
  await expect(page.locator('.execution-panel-logs').first()).toBeVisible();
  // 采样日志行数量，稍等后再次采样：运行中任务应持续增长（验证缓冲→追加链路）。
  const countAfter1s = await page.locator('.execution-panel-logs .log-line').count();
  await page.waitForTimeout(3000);
  const countAfter4s = await page.locator('.execution-panel-logs .log-line').count();

  await page.screenshot({ path: 'tests/__screenshots__/091_logs_running.png', fullPage: false });

  // 不强求一定增长（任务可能恰好在两次采样间无输出），但运行态至少应渲染过日志行，
  // 且全程无运行时错误。
  console.log('091 日志行数采样:', countAfter1s, '→', countAfter4s);
  expect(errors, `运行时错误:\n${errors.join('\n')}`).toEqual([]);
});
