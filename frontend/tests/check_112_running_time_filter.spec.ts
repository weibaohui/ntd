// check_112_running_time_filter.spec.ts — 112：事项执行监控（running）视图顶栏时间过滤回归用例。
//
// 运行方式：
//   本 spec 默认验证 18088 dev 实例（与仓库其他 spec 一致，需 make dev 重建前端产物）；
//   本地开发验证可用 CHECK_BASE_URL=http://localhost:5173 指定独立 vite 实例
//   （vite /api 与 WS 代理到 18088 dev 后端），避免为验证而重启他人正在使用的 dev 实例。
import { test, expect, type Page } from '@playwright/test';

// 验证目标实例：默认 18088（embedded 构建产物）；本地验证用环境变量切换到 vite dev。
const BASE = process.env.CHECK_BASE_URL ?? 'http://localhost:18088';

/** 测量顶栏时间分段与视图切换器的相对位置：时间分段必须与切换器同一行且在其左侧。 */
async function measureHeaderLayout(page: Page) {
  // 顶栏第一个 .ant-segmented 即时间分段（在视图切换器之前渲染，DOM 顺序稳定）；
  // 视图切换器用稳定 data-testid 锚定，避免依赖 DOM 顺序。
  const timeSeg = page.locator('.ant-segmented').first();
  const viewToggle = page.locator('[data-testid="todo-center-view-toggle"]');
  const t = await timeSeg.boundingBox();
  const v = await viewToggle.boundingBox();
  if (!t || !v) throw new Error('顶栏时间分段或视图切换器不可见');
  return { time: t, toggle: v };
}

test('running 视图顶栏渲染时间过滤分段，默认「全部」', async ({ page }) => {
  await page.goto(`${BASE}/#/todos?view=running`);
  // RunningBoard 数据加载完成后出现统计栏（API 经代理打到 dev 后端）
  await expect(page.locator('.running-board-stats')).toBeVisible({ timeout: 20000 });

  // 与卡片/列表同款的时间分段控件出现，「全部」为默认选中项
  await expect(page.getByText('全部', { exact: true })).toBeVisible();
  await expect(page.locator('.ant-segmented-item-selected', { hasText: '全部' }).first()).toBeVisible();
});

test('running 与 card 视图的时间分段位于同一槽位（切换器左侧同一行）', async ({ page }) => {
  // 卡片形态基准：时间分段与视图切换器同行且在其左侧
  await page.goto(`${BASE}/#/todos?view=card`);
  await expect(page.locator('[data-testid="todo-center-view-toggle"]')).toBeVisible();
  const card = await measureHeaderLayout(page);
  expect(card.time.x).toBeLessThan(card.toggle.x);
  // 同一行判定：两个元素纵向区间有交集
  expect(card.time.y).toBeLessThan(card.toggle.y + card.toggle.height);
  expect(card.time.y + card.time.height).toBeGreaterThan(card.toggle.y);

  // running 形态：同槽位（左侧 + 同一行）
  await page.goto(`${BASE}/#/todos?view=running`);
  await expect(page.locator('[data-testid="todo-center-view-toggle"]')).toBeVisible();
  const running = await measureHeaderLayout(page);
  expect(running.time.x).toBeLessThan(running.toggle.x);
  expect(running.time.y).toBeLessThan(running.toggle.y + running.toggle.height);
  expect(running.time.y + running.time.height).toBeGreaterThan(running.toggle.y);
});

test('running 视图切换 24h 后选中态生效并可切回「全部」', async ({ page }) => {
  await page.goto(`${BASE}/#/todos?view=running`);
  await expect(page.locator('.running-board-stats')).toBeVisible({ timeout: 20000 });

  // 切换 24h：Segmented 选中态跟随，同时 hours 已透传 RunningBoard（面板无报错）
  await page.getByText('24h', { exact: true }).click();
  await expect(page.locator('.ant-segmented-item-selected', { hasText: '24h' }).first()).toBeVisible();

  // 切回「全部」恢复默认
  await page.getByText('全部', { exact: true }).click();
  await expect(page.locator('.ant-segmented-item-selected', { hasText: '全部' }).first()).toBeVisible();

  // 截图留证（目录已 gitignore，发布到 PR 评论）
  await page.screenshot({ path: 'tests/__screenshots__/112-running-time-filter.png' });
});

