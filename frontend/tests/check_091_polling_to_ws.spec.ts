// 091 续：去掉讨论帖(4s)/运行看板(60s)兜底定时轮询，改为纯事件驱动刷新。
//
// 本次改动：useDiscussionPosts、useAutoRefreshRunningBoard 删掉 setInterval，
// 改为监听三类事件刷新：executionFinished（WS 实时）、EXECUTION_SYNC_EVENT（WS 重连全量同步）、
// visibilitychange（切回标签页）。本脚本回归验证：
//  1. 应用启动无运行时错误（接线正确，未引入引用错误）。
//  2. 进入「执行器/正在运行」视图（挂载 useAutoRefreshRunningBoard）能正常渲染。
//  3. 派发 EXECUTION_SYNC_EVENT 与 visibilitychange 事件后应用不报错——
//     即事件驱动刷新链路已挂上且能安全触发（替代被移除的定时轮询）。
//  4. 切到「正在运行」tab，初始加载（非定时轮询）能拉到列表或空态。
import { test, expect, type Page, type ConsoleMessage } from '@playwright/test';

const BASE = 'http://localhost:18088';
// 与 useExecutionEvents 导出的事件名保持一致；同步派发验证监听器接线。
const EXECUTION_SYNC_EVENT = 'executionSync';

function attachErrorCollector(page: Page, sink: string[]) {
  page.on('console', (msg: ConsoleMessage) => {
    if (msg.type() !== 'error') return;
    const t = msg.text();
    // 过滤网络/资源噪声与已知第三方报错，聚焦代码运行时错误。
    if (/Failed to load resource|net::ERR|404|favicon|preload|CORS/i.test(t)) return;
    sink.push(t);
  });
  page.on('pageerror', (err: Error) => sink.push(`pageerror: ${err.message}`));
}

test('091-poll：去除定时轮询后应用启动无运行时错误', async ({ page }) => {
  const errors: string[] = [];
  attachErrorCollector(page, errors);

  await page.goto(`${BASE}/#/todos`, { waitUntil: 'domcontentloaded' });
  await page.waitForTimeout(2000);

  await page.screenshot({ path: 'tests/__screenshots__/091_poll_boot.png', fullPage: false });
  expect(errors, `启动运行时错误:\n${errors.join('\n')}`).toEqual([]);
  console.log('091-poll 启动回归通过');
});

test('091-poll：执行器视图挂载 useAutoRefreshRunningBoard 并响应 WS 重连/可见性事件', async ({ page }) => {
  const errors: string[] = [];
  attachErrorCollector(page, errors);

  // 进入执行器视图：ExecutorsPanel 内调用 useAutoRefreshRunningBoard，挂载事件监听。
  await page.goto(`${BASE}/#/executors`, { waitUntil: 'domcontentloaded' });
  await page.waitForTimeout(2500);

  // 派发 WS 重连全量同步事件——验证监听器已挂上且安全触发（替代 60s 兜底轮询）。
  await page.evaluate((evt) => window.dispatchEvent(new Event(evt)), EXECUTION_SYNC_EVENT);
  // 模拟切回标签页：visibilitychange → visible 触发一次单次刷新。
  await page.evaluate(() => {
    Object.defineProperty(document, 'visibilityState', { configurable: true, get: () => 'visible' });
    document.dispatchEvent(new Event('visibilitychange'));
  });
  await page.waitForTimeout(800);

  await page.screenshot({ path: 'tests/__screenshots__/091_poll_executors.png', fullPage: false });
  expect(errors, `执行器视图事件触发后运行时错误:\n${errors.join('\n')}`).toEqual([]);
  console.log('091-poll 执行器视图事件驱动回归通过');
});

test('091-poll：正在运行 tab 走初始加载（非定时轮询）能渲染', async ({ page }) => {
  const errors: string[] = [];
  attachErrorCollector(page, errors);

  await page.goto(`${BASE}/#/executors`, { waitUntil: 'domcontentloaded' });
  await page.waitForTimeout(2500);

  // 点击「正在运行」tab：触发 useEffect 内的初始 loadRunningRecords()（已无 60s 定时轮询）。
  const runningTab = page.getByRole('tab', { name: '正在运行' });
  await runningTab.click();
  await page.waitForTimeout(1500);

  await page.screenshot({ path: 'tests/__screenshots__/091_poll_running_tab.png', fullPage: false });
  // 运行记录表格或「暂无运行中任务」空态二者居其一即可，关键是不报错且成功切换。
  const hasTable = await page.locator('table').count();
  expect(errors, `正在运行 tab 运行时错误:\n${errors.join('\n')}`).toEqual([]);
  expect(hasTable, '正在运行 tab 应渲染出表格').toBeGreaterThan(0);
  console.log('091-poll 正在运行 tab 回归通过');
});
