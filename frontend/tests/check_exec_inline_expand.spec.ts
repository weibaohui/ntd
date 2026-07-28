// 调试脚本：验证执行历史改为「内联手风琴」后的交互。
// 点某个执行项 → 看板应在该项正下方、同框展开。
import { test, expect } from '@playwright/test';

const BASE = 'http://localhost:5180';

test('执行历史内联展开校验', async ({ page }) => {
  await page.goto(`${BASE}/#/tasks`, { waitUntil: 'domcontentloaded' });

  // 进入详情：点列表第一行。
  await page.waitForSelector('.ant-table-row', { timeout: 30000 });
  await page.locator('.ant-table-row').first().click();

  // 切到执行历史 Tab。
  await page.getByText('执行历史', { exact: false }).first().click();
  await expect(page.locator('.ant-tabs-tabpane-active')).toBeVisible();

  // 第一个折叠项（aria-label 含 查看详情）。
  const collapsedItem = page.locator('[role="button"][aria-label*="查看详情"]').first();
  await expect(collapsedItem).toBeVisible();

  // 点击前：该折叠项所属卡片内不应包含看板。
  const preCard = collapsedItem.locator('xpath=ancestor::div[contains(@class, "execItem")][1]');
  await expect(preCard.getByText('执行看板', { exact: false })).toHaveCount(0);

  // 点击展开。
  await collapsedItem.click();

  // 重新定位「已展开」项（aria-label 变为 收起详情），再断言其卡片内包含看板 + execDetail 框。
  const expandedItem = page.locator('[role="button"][aria-label*="收起详情"]').first();
  await expect(expandedItem).toBeVisible();
  const expandedCard = expandedItem.locator('xpath=ancestor::div[contains(@class, "execItem")][1]');

  // 看板出现在同一卡片内（内联、同框）。
  await expect(expandedCard.getByText('执行看板', { exact: false })).toBeVisible({ timeout: 15000 });
  // 详情框（execDetail）也在同一卡片内，验证整体单元结构。
  await expect(expandedCard.locator('[class*="execDetail"]')).toBeVisible();

  // 截图核对：表头正下方、同框的看板。
  await page.screenshot({ path: 'tests/__screenshots__/exec_inline_expand.png', fullPage: false });

  console.log('执行历史内联展开校验通过：看板在点击项正下方同框展开');
});