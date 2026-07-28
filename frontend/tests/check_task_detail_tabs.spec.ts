// 调试脚本：验证任务详情页 Tabs 重构后的布局与交互。
// 不进入正式回归，仅用于本次 UI 重设计的视觉/结构核对。
import { test, expect } from '@playwright/test';

const BASE = 'http://localhost:18088';

test('任务详情页 Tabs 布局校验', async ({ page }) => {
  // 进入任务列表页（默认 list 视图为 Table）。
  await page.goto(`${BASE}/#/tasks`, { waitUntil: 'networkidle' });

  // 等列表表格渲染，点第一行进入详情。
  await page.waitForSelector('.ant-table-row', { timeout: 15000 });
  await page.locator('.ant-table-row').first().click();

  // 详情态应出现新的 Tabs 容器（顶部条 + Tabs 两区布局）。
  const tabs = page.locator('.ant-tabs');
  await expect(tabs.first()).toBeVisible({ timeout: 15000 });

  // 三个 Tab 标签齐全：概览 / 工艺要求 / 执行历史。
  await expect(page.getByText('概览', { exact: true }).first()).toBeVisible();
  await expect(page.getByText('工艺要求', { exact: false }).first()).toBeVisible();
  await expect(page.getByText('执行历史', { exact: false }).first()).toBeVisible();

  // 顶部条「再次执行」主按钮始终可见。
  await expect(page.getByRole('button', { name: '再次执行' })).toBeVisible();

  // 截图：概览 Tab 默认态
  await page.screenshot({ path: 'tests/__screenshots__/task_detail_overview.png', fullPage: false });

  // 切到「工艺要求」：应渲染轻量步骤列表（序号徽标）。
  await page.getByText('工艺要求', { exact: false }).first().click();
  await expect(page.locator('.ant-tabs-tabpane-active')).toBeVisible();
  await page.screenshot({ path: 'tests/__screenshots__/task_detail_process.png', fullPage: false });

  // 切到「执行历史」：渲染执行列表。
  await page.getByText('执行历史', { exact: false }).first().click();
  await expect(page.locator('.ant-tabs-tabpane-active')).toBeVisible();
  await page.screenshot({ path: 'tests/__screenshots__/task_detail_exec.png', fullPage: false });

  console.log('任务详情页 Tabs 布局校验通过');
});
