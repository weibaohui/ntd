import { test, expect } from '@playwright/test';

test('斜杠命令添加弹窗 Todo 下拉', async ({ page }) => {
  await page.goto('http://localhost:18088');
  await page.waitForLoadState('networkidle');
  await page.waitForTimeout(2000);

  // 更多 → 设置
  await page.locator('.header-overflow-btn').click();
  await page.waitForTimeout(800);
  await page.locator('.ant-dropdown-menu-item').filter({ hasText: '设置' }).click();
  await page.waitForTimeout(1500);

  // 工作空间 tab → xyz 进入
  await page.locator('.ant-tabs-tab').filter({ hasText: '工作空间' }).click();
  await page.waitForTimeout(1500);
  await page.getByRole('button', { name: 'right' }).nth(3).click();
  await page.waitForTimeout(1500);

  // 斜杠命令 tab → 添加
  await page.locator('.ant-tabs-tab').filter({ hasText: '斜杠命令' }).click();
  await page.waitForTimeout(1000);
  await page.locator('button').filter({ hasText: '添加斜杠命令' }).click();
  await page.waitForTimeout(2000);

  // 点第一个 select（绑定 Todo 下拉）
  const select = page.locator('[role="dialog"] .ant-select').first();
  const selectVisible = await select.isVisible();
  console.log('Select visible:', selectVisible);
  if (selectVisible) {
    await select.click();
    await page.waitForTimeout(800);
    const opts = page.locator('.ant-select-item-option-content');
    const optCnt = await opts.count();
    console.log('Todo 选项数:', optCnt);
    await page.screenshot({ path: 'todo_dropdown.png' });
  }
});
