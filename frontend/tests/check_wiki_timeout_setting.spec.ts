/**
 * Wiki 执行超时设置验证：
 *
 * 背景：blackboard.rs 的 wait_for_finished / spawn_executor_for_chat_streaming
 * 原本把超时写死成 300 秒，用户无法调整。现已把超时做成 per-workspace
 * 可配置项（wiki_timeout_secs），暴露在黑板设置弹窗的「防抖设置」Tab 中。
 *
 * 本脚本验证：
 * 1. 打开黑板设置弹窗，能在「防抖设置」Tab 看到「Wiki 执行超时」输入框
 * 2. 输入框可修改（填入 600）
 * 3. 保存后弹窗关闭，提示「设置已保存」
 * 4. 重新打开弹窗，输入框回显刚才保存的 600（配置已持久化）
 * 5. 恢复默认 300 并保存，避免污染后续测试
 */
import { test, expect } from '@playwright/test';

/** 防抖设置 Tab 中「Wiki 执行超时」对应的 InputNumber 在 DOM 中的序号。
 * 防抖设置 Tab 自上而下依次为：防抖周期(0) / 触发条数(1) / Wiki 执行超时(2)。 */
const WIKI_TIMEOUT_INPUT_INDEX = 2;

test('Wiki 执行超时可在黑板设置界面配置并持久化', async ({ page }) => {
  // 用 hash 路由直接进入黑板页面（与 useViewState 的 hash 路由对齐）
  await page.goto('http://localhost:18088/#/blackboard?workspace=1');
  await page.waitForSelector('#root > *', { timeout: 15000 });
  await page.waitForSelector('button[title="设置"]', { timeout: 10000 });

  // 打开设置弹窗
  await page.click('button[title="设置"]');
  await expect(page.locator('.ant-modal')).toBeVisible({ timeout: 5000 });

  // 第三个 InputNumber 即「Wiki 执行超时」，验证它已上界面
  const timeoutInput = page.locator('.ant-input-number-input').nth(WIKI_TIMEOUT_INPUT_INDEX);
  await expect(timeoutInput).toBeVisible();

  // 修改为 600 秒并保存
  await timeoutInput.fill('600');
  await timeoutInput.blur();
  await expect(timeoutInput).toHaveValue('600');

  // 点击保存
  await page.locator('.ant-modal-footer .ant-btn-primary').first().click();
  await expect(page.locator('.ant-message')).toContainText('设置已保存', { timeout: 5000 });
  await expect(page.locator('.ant-modal')).not.toBeVisible({ timeout: 5000 });

  // 重新打开弹窗，验证配置已持久化（回显 600）
  await page.click('button[title="设置"]');
  await expect(page.locator('.ant-modal')).toBeVisible({ timeout: 5000 });
  const timeoutInputReopen = page.locator('.ant-input-number-input').nth(WIKI_TIMEOUT_INPUT_INDEX);
  await expect(timeoutInputReopen).toHaveValue('600');

  // 恢复默认 300 并保存，避免污染其他测试用例
  await timeoutInputReopen.fill('300');
  await timeoutInputReopen.blur();
  await page.locator('.ant-modal-footer .ant-btn-primary').first().click();
  await expect(page.locator('.ant-message')).toContainText('设置已保存', { timeout: 5000 });
});
