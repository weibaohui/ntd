// 091 续：看板 / 讨论区去轮询后补「手动刷新」按钮（Commit 8）。
//
// 纯 WS 事件驱动后，断线或 WS 长时间无响应时用户可自行重拉——本脚本回归验证两处刷新按钮：
//  1. 运行看板（/#/memorial?mode=running）stats bar 末尾「刷新」按钮存在、可点、点击触发重拉无运行时错误。
//  2. 任务讨论区（/#/tasks/<id>）顶部「刷新」按钮存在、可点、点击触发重拉无运行时错误。
//
// 两个按钮均复用各自 hook 的 loading 态驱动转圈；点击后断言页面无 JS 运行时错误，确认接线正确。
import { test, expect, type Page, type ConsoleMessage } from '@playwright/test';

const BASE = 'http://localhost:18088';

/** 收集页面运行时错误：console.error（过滤网络/资源噪声）+ 未捕获 pageerror。 */
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

test('091-refresh：运行看板 stats bar「刷新」按钮可点击无错误', async ({ page }) => {
  const errors: string[] = [];
  attachErrorCollector(page, errors);

  // 运行视图：挂载 RunningBoard，stats bar 末尾带刷新按钮（复用 useRunningBoard.refresh）。
  await page.goto(`${BASE}/#/memorial?mode=running`, { waitUntil: 'domcontentloaded' });
  // 等初始 loadRunningRecords 完成（loading 从 true→false）后 stats bar 才进入主渲染分支。
  await page.waitForTimeout(2500);

  // 用 .running-board-stats 容器作用域限定，避免与页面其他「刷新」重名按钮误匹配。
  const refreshBtn = page.locator('.running-board-stats').getByRole('button', { name: '刷新' });
  await expect(refreshBtn).toBeVisible();
  await refreshBtn.click();
  // refresh() 内部 setLoading(true)→拉取→setLoading(false)，留时间让请求与转圈落地。
  await page.waitForTimeout(1500);

  await page.screenshot({ path: 'tests/__screenshots__/091_refresh_running_board.png', fullPage: false });
  expect(errors, `运行看板刷新后运行时错误:\n${errors.join('\n')}`).toEqual([]);
  console.log('091-refresh 运行看板刷新按钮回归通过');
});

test('091-refresh：任务讨论区顶部「刷新」按钮可点击无错误', async ({ page }) => {
  const errors: string[] = [];
  attachErrorCollector(page, errors);

  // task 39 存在且 task_posts 有 3 条；DiscussionTab forceRender 即便非 active 也已挂载，
  // 但切到「讨论」tab 让其成为 active tabpane，刷新按钮才可见可点。
  await page.goto(`${BASE}/#/tasks/39`, { waitUntil: 'domcontentloaded' });
  await page.waitForTimeout(2500);

  // 切到「讨论」tab（label 是 Badge 包裹的「讨论」，用正则匹配其可访问名）。
  await page.getByRole('tab', { name: /讨论/ }).click();
  await page.waitForTimeout(800);

  // 作用域到 active tabpane，确保命中的是讨论区的刷新按钮（与其他 tab 内可能存在的刷新区分）。
  const refreshBtn = page.locator('.ant-tabs-tabpane-active').getByRole('button', { name: '刷新' });
  await expect(refreshBtn).toBeVisible();
  await refreshBtn.click();
  await page.waitForTimeout(1500);

  await page.screenshot({ path: 'tests/__screenshots__/091_refresh_discussion.png', fullPage: false });
  expect(errors, `讨论区刷新后运行时错误:\n${errors.join('\n')}`).toEqual([]);
  console.log('091-refresh 讨论区刷新按钮回归通过');
});
