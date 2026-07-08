/**
 * 黑板设置弹窗验证：
 * 1. 直接导航到黑板页面
 * 2. 点击设置按钮能打开弹窗
 * 3. 防抖周期和触发条数 InputNumber 正常显示和修改
 * 4. Wiki 执行超时 InputNumber 正常显示和修改（per-workspace 可配）
 * 5. 切换到提示词设置 Tab，验证 TextArea 和恢复默认按钮
 * 6. 保存后弹窗关闭并提示成功
 */
import { test, expect } from '@playwright/test';

test('黑板设置弹窗能正常打开和保存', async ({ page }) => {
  // 直接导航到黑板页面（hash 路由：#/blackboard?workspace=1）
  await page.goto('http://localhost:18088/#/blackboard?workspace=1');
  // 等待 app 渲染完成
  await page.waitForSelector('#root > *', { timeout: 15000 });

  // 等待设置按钮可见
  await page.waitForSelector('button[title="设置"]', { timeout: 10000 });

  // 点击设置按钮
  await page.click('button[title="设置"]');

  // 弹窗应该出现
  await expect(page.locator('.ant-modal')).toBeVisible({ timeout: 5000 });

  // 防抖输入框应该可见（默认在防抖设置 Tab）
  const input = page.locator('.ant-input-number-input').first();
  await expect(input).toBeVisible();

  // 修改防抖时间为 300
  await input.fill('300');
  await input.blur();

  // Wiki 执行超时输入框应在防抖设置 Tab 中可见（防抖周期 / 触发条数 / Wiki 执行超时）
  // 验证第三个 InputNumber 存在且可修改，确认超时设置已上界面
  const timeoutInput = page.locator('.ant-input-number-input').nth(2);
  await expect(timeoutInput).toBeVisible();
  await timeoutInput.fill('600');
  await timeoutInput.blur();
  await expect(timeoutInput).toHaveValue('600');

  // 切换到提示词设置 Tab
  await page.click('.ant-tabs-tab:has-text("提示词设置")');

  // 更新提示词 TextArea 应该可见
  const updatePromptArea = page.locator('textarea').first();
  await expect(updatePromptArea).toBeVisible();

  // 点击恢复默认按钮
  const restoreBtn = page.locator('button:has-text("恢复默认")');
  await expect(restoreBtn).toBeVisible();
  await restoreBtn.click();

  // 输入框应被填入默认提示词内容
  await expect(updatePromptArea).not.toHaveValue('');

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
