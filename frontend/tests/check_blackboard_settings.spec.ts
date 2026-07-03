/**
 * 黑板设置弹窗验证：
 * 1. 直接导航到黑板页面
 * 2. 点击设置按钮能打开弹窗
 * 3. 防抖时间 InputNumber 正常显示和修改
 * 4. 保存后弹窗关闭并提示成功
 */
import { test, expect } from '@playwright/test';

test('黑板设置弹窗能正常打开和保存', async ({ page }) => {
  // 直接导航到黑板页面
  await page.goto('http://localhost:18088/?view=blackboard&workspace=1');
  // 等待 app 渲染完成
  await page.waitForSelector('#root > *', { timeout: 15000 });

  // 等待设置按钮可见
  await page.waitForSelector('button[title="设置"]', { timeout: 10000 });

  // 点击设置按钮
  await page.click('button[title="设置"]');

  // 弹窗应该出现
  await expect(page.locator('.ant-modal')).toBeVisible({ timeout: 5000 });

  // 防抖输入框应该可见
  const input = page.locator('.ant-input-number-input').first();
  await expect(input).toBeVisible();

  // 修改防抖时间为 300
  await input.fill('300');
  await input.blur();

  // 等待一下让表单更新
  await page.waitForTimeout(500);

  // 点击弹窗底部的确认按钮（primary button in footer）
  const saveButton = page.locator('.ant-modal-footer .ant-btn-primary').first();
  await expect(saveButton).toBeVisible();
  await saveButton.click();

  // 应该提示成功
  await expect(page.locator('.ant-message')).toContainText('设置已保存', { timeout: 5000 });

  // 弹窗应该关闭
  await expect(page.locator('.ant-modal')).not.toBeVisible({ timeout: 5000 });
});
