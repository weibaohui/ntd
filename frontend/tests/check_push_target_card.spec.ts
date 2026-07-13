/**
 * 推送目标简化 — PushStatusCard UI 验证
 *
 * 验证点：
 *   1. 群ID 行已移除（不再展示群聊推送目标）
 *   2. 「单聊ID」旧 label 已移除
 *   3. 改为「推送目标」展示 owner_open_id（所有者，自动捕获）
 *
 * 用 mount harness 把 PushStatusCard 单独挂起来，避免完整应用的登录/路由依赖。
 */
import { test, expect } from '@playwright/test';

const HARNESS_URL = 'http://localhost:5173/tests/push-target-mount.html';

test('PushStatusCard: 群ID已移除，改为推送目标(所有者)展示', async ({ page }) => {
  await page.goto(HARNESS_URL);
  // 等待 mount 完成（成功或失败都会置 __renderDone）
  await page.waitForFunction(() => (window as any).__renderDone === true, { timeout: 10000 });

  // mount 不应有渲染异常
  const renderError = await page.evaluate(() => (window as any).__renderError);
  expect(renderError, `mount 渲染失败: ${renderError}`).toBeUndefined();

  const body = (await page.textContent('body')) || '';

  // 新行为：展示推送目标（所有者 open_id）
  expect(body).toContain('推送目标');
  // antd Input 的 value 不进 textContent，用 locator 断言输入框展示了 owner_open_id
  await expect(page.locator('input[value*="ou_b0cb04a51dd7075e92341fbcbde944cd"]')).toBeVisible();

  // 旧行为应已移除：群ID 行、单聊ID label
  expect(body).not.toContain('群ID:');
  expect(body).not.toContain('单聊ID:');
});
