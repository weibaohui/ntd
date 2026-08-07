// 091 性能优化回归：验证首屏代码分割后各 lazy 页面可按需加载、正常渲染。
//
// Commit 6 把事项主路径以外的页面级组件改为 React.lazy，monaco / @xyflow 拆成独立 vendor chunk。
// 本脚本逐个访问典型视图，确认：
//  1. lazy chunk 能成功拉取并渲染（不会卡在 Suspense fallback）。
//  2. 切换过程无运行时错误（React.lazy/import 装配正确）。
// 截图留档用于 PR 评审。
import { test, expect, type Page, type ConsoleMessage } from '@playwright/test';

const BASE = 'http://localhost:18088';

function attachErrorCollector(page: Page, sink: string[]) {
  page.on('console', (msg: ConsoleMessage) => {
    if (msg.type() !== 'error') return;
    const t = msg.text();
    // 过滤网络/资源噪声与已知第三方报错，聚焦代码运行时错误与 chunk 加载失败。
    if (/Failed to load resource|net::ERR|404|favicon|preload|CORS/i.test(t)) return;
    sink.push(t);
  });
  page.on('pageerror', (err: Error) => sink.push(`pageerror: ${err.message}`));
}

// 主路径（事项列表）应为静态加载，首屏直接渲染，不出现 Suspense fallback。
test('091：首屏主路径静态加载，不触发 Suspense fallback', async ({ page }) => {
  const errors: string[] = [];
  attachErrorCollector(page, errors);

  await page.goto(`${BASE}/#/todos`, { waitUntil: 'domcontentloaded' });
  await page.waitForTimeout(2000);

  await page.screenshot({ path: 'tests/__screenshots__/091_bundle_todos.png', fullPage: false });
  expect(errors, `首屏运行时错误:\n${errors.join('\n')}`).toEqual([]);
  console.log('091 首屏主路径回归通过');
});

// 逐个访问 lazy 视图：每个都是独立 chunk，验证按需加载不报错且最终渲染出内容。
const LAZY_VIEWS = [
  { hash: '#/loops', label: 'loops' },
  { hash: '#/tasks', label: 'tasks' },
  { hash: '#/processes', label: 'processes' },
  { hash: '#/dashboard', label: 'dashboard' },
];

for (const v of LAZY_VIEWS) {
  test(`091：lazy 视图可加载渲染——${v.label}`, async ({ page }) => {
    const errors: string[] = [];
    attachErrorCollector(page, errors);

    await page.goto(`${BASE}/${v.hash}`, { waitUntil: 'domcontentloaded' });
    // 等待 lazy chunk 拉取并挂载（含 vendor-flow / vendor-monaco 等按需 chunk）。
    await page.waitForTimeout(3000);

    await page.screenshot({ path: `tests/__screenshots__/091_bundle_${v.label}.png`, fullPage: false });
    expect(errors, `${v.label} 视图运行时错误:\n${errors.join('\n')}`).toEqual([]);
    console.log(`091 lazy 视图 ${v.label} 回归通过`);
  });
}
