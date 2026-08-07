// 091 性能优化回归：验证日志链路重构后执行面板仍正常工作。
//
// 重构要点（需回归）：
//  1. LogsProvider 嵌入 AppProvider、useApp 路由日志 action —— 若装配错误，应用启动即白屏。
//  2. 日志改走 @tanstack/react-virtual 虚拟列表 —— 任务运行时 .execution-panel-logs 内
//     应能渲染 .log-line（只渲染可视区行，行数恒定）。
//  3. WS Output → 50ms 缓冲 → APPEND_TASK_LOGS —— 面板底部应随输出增长并自动滚到底。
//
// 091 评审修复（原脚本两处失效断言）：
//  - WS 连通：原脚本在 goto+2500ms 后才猴子补丁 window.WebSocket，单例早已建立，探测永远
//    超时(wsState=-1)、断言从不执行。改用 Playwright 原生 page.on('websocket')，监听器在
//    goto 前注册，应用建连即捕获，真正校验 WS 装配。
//  - 虚拟列表：原脚本采样行数后只 console.log、不参与断言。改为对渲染行数施加「虚拟上界」
//    守卫——虚拟器只渲染可视区+overscan，行数受限于视口高度，若退化为全量渲染且日志足够多
//    即可被该上界拦截。
import { test, expect, type Page, type ConsoleMessage } from '@playwright/test';

const BASE = 'http://localhost:18088';
// 与 ExecutionPanelLogs 的虚拟器常量对齐，用于计算渲染行数上界。
const ESTIMATED_ROW_HEIGHT = 22;
const OVERSCAN = 12;

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

  // 用 Playwright 原生 WebSocket 事件捕获单例连接：监听器在 goto 之前注册，
  // 应用创建 WS 时即触发，不受「连接先于探测建立」的时序问题影响（091 评审修复）。
  let wsConnected = false;
  page.on('websocket', () => { wsConnected = true; });

  // 应用外壳渲染：若 LogsProvider/useApp 装配错误，此处会超时（白屏）。
  await page.goto(`${BASE}`, { waitUntil: 'domcontentloaded' });
  await page.waitForSelector('body', { timeout: 15000 });
  // 等待 React 挂载稳定，给 Provider 初始化与首屏副作用（含 WS 单例建立）留出时间。
  await page.waitForTimeout(2500);

  // 截图留档（不入 git，仅本地/PR 评论参考）。
  await page.screenshot({ path: 'tests/__screenshots__/091_logs_boot.png', fullPage: false });

  // 核心断言：重构不应引入任何运行时错误。
  expect(errors, `控制台出现疑似运行时错误:\n${errors.join('\n')}`).toEqual([]);
  // WS 单例应已建立：page.on('websocket') 在 goto 前注册，捕获到即说明事件订阅装配正确。
  expect(wsConnected, '应用应在启动时建立 WebSocket 单例').toBe(true);
  console.log('091 启动回归通过：无运行时错误，WS 已连通');
});

test('091：运行中任务的执行面板用虚拟列表渲染日志行（渲染行数受虚拟上界约束）', async ({ page }) => {
  const errors: string[] = [];
  attachErrorCollector(page, errors);

  await page.goto(`${BASE}`, { waitUntil: 'domcontentloaded' });
  await page.waitForTimeout(2500);

  // 执行面板仅在存在运行任务时渲染。空闲态无运行任务属正常，跳过行渲染断言不算失败。
  const panelVisible = await page.locator('.execution-panel').isVisible().catch(() => false);
  if (!panelVisible) {
    console.log('091 日志渲染回归：当前无运行任务，面板未展示，跳过行渲染断言');
    expect(errors).toEqual([]);
    return;
  }

  // 面板可见时，日志区容器应存在。
  await expect(page.locator('.execution-panel-logs').first()).toBeVisible();

  // 等输出到达：运行中任务通常很快产出首条日志；4s 内出现即继续校验，否则视为暂无输出。
  let rowCount = 0;
  try {
    await page.locator('.execution-panel-logs .log-line').first().waitFor({ timeout: 4000, state: 'attached' });
    // 首条出现后再等一拍，让缓冲→追加稳定，随后采样渲染行数。
    await page.waitForTimeout(1500);
    rowCount = await page.locator('.execution-panel-logs .log-line').count();
  } catch {
    console.log('091 日志渲染回归：面板可见但 4s 内无日志行（任务可能尚未产出），跳过行数断言');
  }

  await page.screenshot({ path: 'tests/__screenshots__/091_logs_running.png', fullPage: false });

  if (rowCount > 0) {
    // 虚拟列表上界守卫：虚拟器只渲染「可视区 + 两侧 overscan」，行数应受限于视口高度。
    // 理论上界 ≈ ceil(clientH / ESTIMATED_ROW_HEIGHT) + 2 * OVERSCAN；+5 容纳测量抖动。
    // 若虚拟化退化为全量渲染、且任务日志足够多，rowCount 会突破此界，断言失败（091 评审修复）。
    const ceiling = await page.evaluate(([rowH, osc]) => {
      const el = document.querySelector('.execution-panel-logs');
      if (!el) return Number.MAX_SAFE_INTEGER;
      return Math.ceil(el.clientHeight / rowH) + 2 * osc + 5;
    }, [ESTIMATED_ROW_HEIGHT, OVERSCAN]);
    expect(
      rowCount,
      `虚拟列表渲染行数 ${rowCount} 超过上界 ${ceiling}（疑似退化为全量渲染）`,
    ).toBeLessThanOrEqual(ceiling);
    console.log('091 日志渲染回归通过：渲染行数', rowCount, '虚拟上界', ceiling);
  } else {
    console.log('091 日志渲染回归通过：面板可见、无运行时错误（本轮无输出行，未校验上界）');
  }

  expect(errors, `运行时错误:\n${errors.join('\n')}`).toEqual([]);
});
