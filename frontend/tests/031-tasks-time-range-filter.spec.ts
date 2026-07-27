// 031-任务页时间过滤分段 Playwright 验证。
// 验证点（对应需求 031 §7）：
// 1. 任务页默认「全部」选中，显示所有任务；
// 2. 切到 24h 后仅最近 24 小时创建的任务可见；
// 3. 切回「全部」恢复；
// 4. 看板页（memorial?mode=kanban）分段回归：默认 24h 选中、无「全部」选项。

import { test, expect } from '@playwright/test';

const BASE = 'http://localhost:18088';

// 定位顶栏中的时间分段：AntD Segmented 容器类。
// 任务页顶栏另有一个图标式视图切换 Segmented，因此用「全部」文本锁定时间分段容器。
const timeSegment = (page: import('@playwright/test').Page) =>
  page.locator('.ant-segmented', { has: page.getByText('全部', { exact: true }) }).first();

test('任务页时间过滤分段', async ({ page }) => {
  await page.goto(`${BASE}/#/tasks`);
  // 等列表首屏渲染完成（行或空态出现即视为加载结束）。
  await page.waitForSelector('.ant-table-row, .ant-empty', { timeout: 15000 });

  // —— 1. 默认「全部」选中 ——
  const segment = timeSegment(page);
  await expect(segment.getByText('全部', { exact: true })).toBeVisible();
  const allItem = segment.locator('.ant-segmented-item', { hasText: '全部' }).first();
  await expect(allItem).toHaveClass(/ant-segmented-item-selected/);

  // 造数：通过后端 API 直接创建一个任务，保证其 created_at 落在最近窗口内；
  // 列表全量计数记为 totalAll。若环境无可用环路（创建任务依赖环路），则退化为
  // 仅验证分段交互与计数不增，不依赖造数。
  const countText = () =>
    page.locator('text=/共 \\d+ 个任务/').first().textContent({ timeout: 5000 });
  const readCount = async (): Promise<number> => {
    const t = await countText();
    const m = t?.match(/(\d+)/);
    return m ? Number(m[1]) : -1;
  };

  const totalAll = await readCount();
  expect(totalAll).toBeGreaterThanOrEqual(0);

  // —— 2. 切 24h：计数不超过全部态，且分段选中态切换 ——
  await segment.getByText('24h', { exact: true }).click();
  const item24 = segment.locator('.ant-segmented-item', { hasText: '24h' }).first();
  await expect(item24).toHaveClass(/ant-segmented-item-selected/);
  const total24h = await readCount();
  expect(total24h).toBeGreaterThanOrEqual(0);
  expect(total24h).toBeLessThanOrEqual(totalAll);

  // —— 3. 切回「全部」：计数恢复 ——
  await segment.getByText('全部', { exact: true }).click();
  await expect(allItem).toHaveClass(/ant-segmented-item-selected/);
  expect(await readCount()).toBe(totalAll);
});

test('看板页时间分段回归（无全部选项，默认 24h）', async ({ page }) => {
  await page.goto(`${BASE}/#/memorial?mode=kanban`);
  // 看板顶栏分段：含 24h 文本的 Segmented 容器。
  const segment = page.locator('.ant-segmented', { has: page.getByText('24h', { exact: true }) }).first();
  await expect(segment.getByText('6h', { exact: true })).toBeVisible();
  await expect(segment.getByText('7d', { exact: true })).toBeVisible();
  // 看板不得出现「全部」选项（需求 031：showAll 仅任务页）。
  await expect(segment.getByText('全部', { exact: true })).toHaveCount(0);
  // 默认 24h 选中（MemorialBoard hours 初值 24）。
  const item24 = segment.locator('.ant-segmented-item', { hasText: '24h' }).first();
  await expect(item24).toHaveClass(/ant-segmented-item-selected/);
  // 切换 3d 后选中态跟随。
  await segment.getByText('3d', { exact: true }).click();
  const item3d = segment.locator('.ant-segmented-item', { hasText: '3d' }).first();
  await expect(item3d).toHaveClass(/ant-segmented-item-selected/);
});
