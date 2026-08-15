// 107：闪念捕捉 UI 回归（功能清单 F2）。
// 覆盖：
// 1. 点击 FAB「闪念捕捉」弹窗出现（标题/输入框/稍后/立即执行按钮）；
// 2. 输入内容点「稍后」→「已创建任务」、弹窗关闭；
// 3. 事项列表搜索命中新建事项（真实落库验证）；
// 4. 清理：删除测试数据，保证可重复执行。
//
// 运行前提：make dev 已起（18088）。注意 antd Tooltip 残留遮挡坑：点击 FAB 前移除。

import { test, expect } from '@playwright/test';

const BASE = 'http://localhost:18088';

test('F2 闪念捕捉：弹窗→创建→列表命中→清理', async ({ page }) => {
  const stamp = `CDP闪念回归${Date.now() % 100000}`;

  await page.goto(`${BASE}/#/todos`);
  await page.waitForSelector('.ant-segmented-item, main', { timeout: 15000 });
  // 环境坑防护：移除 antd Tooltip 残留（会遮挡 FAB 点击）。
  await page.evaluate(() => document.querySelectorAll('.ant-tooltip-container').forEach((el) => el.remove()));

  // —— 1. 弹窗出现 ——
  await page.getByRole('button', { name: '闪念捕捉' }).click();
  const modal = page.locator('.ant-modal:visible');
  await expect(modal).toBeVisible({ timeout: 8000 });
  await expect(modal.locator('.ant-modal-title')).toHaveText('闪念捕捉');
  await expect(modal.locator('textarea')).toBeVisible();

  // —— 2. 输入并「稍后」保存 ——
  await modal.locator('textarea').first().fill(`闪念内容：${stamp}`);
  await modal.locator('button').filter({ hasText: /稍\s*后/ }).click();
  await expect(page.locator('.ant-message')).toContainText('已创建', { timeout: 8000 });
  await expect(modal).toHaveCount(0, { timeout: 8000 });

  // —— 3. 列表命中 ——
  const search = page.locator('input[placeholder*="搜索"]').first();
  await search.fill(stamp);
  await expect(page.locator('main')).toContainText('闪念内容', { timeout: 8000 });

  // —— 4. 清理：搜索框清空，回到列表视图打开该事项并删除 ——
  await search.fill('');
  await page.waitForTimeout(800);
  // 详情删除：点该事项 → 删除 → 确认
  await page.locator('main').getByText('闪念内容', { exact: false }).first().click({ force: true });
  await page.waitForURL(/#\/todos\/\d+/, { timeout: 10000 });
  const delBtn = page.getByRole('button', { name: /删\s*除/ }).first();
  await expect(delBtn).toBeVisible({ timeout: 8000 });
  await delBtn.click({ force: true });
  // 确认弹层（Popover 或 Modal）
  const confirm = page.locator('.ant-popover:visible, .ant-modal:visible').filter({ hasText: /删\s*除|确\s*认/ }).first();
  if (await confirm.count()) {
    await confirm.getByRole('button').last().click({ force: true });
  } else {
    await page.keyboard.press('Enter');
  }
  await expect(page.locator('.ant-message')).toContainText('删除成功', { timeout: 8000 });
});
