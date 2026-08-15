// 107：Wiki 对话浮窗 UI 回归（功能清单 F3，不触发真实执行）。
// 覆盖：
// 1. 点击 FAB「对话」→ 右侧浮窗出现（标题「对话」+ 空态文案 + 输入提示）；
// 2. 浮窗含工作空间选择与执行器选择；
// 3. 输入框可输入（不发送，真实执行链路由 F3 执行记录人工验证）。
//
// 运行前提：make dev 已起（18088）。浮窗为内联样式 fixed 容器（无特征 class），
// 用「right: 0px + width>300」特征定位；FAB 按钮可能被 Tooltip 残留遮挡，点击前移除。

import { test, expect } from '@playwright/test';

const BASE = 'http://localhost:18088';

/** 定位右侧 WikiChat 浮窗：内联 style 含 position: fixed 且包含空态文案的容器。 */
function chatWindow(page: import('@playwright/test').Page) {
  return page
    .locator('div[style*="position: fixed"]')
    .filter({ hasText: '还没有对话记录' })
    .first();
}

test('F3 对话浮窗：FAB 打开、空态渲染、输入框可用', async ({ page }) => {
  await page.goto(`${BASE}/#/todos`);
  await page.waitForSelector('main', { timeout: 15000 });
  await page.evaluate(() => document.querySelectorAll('.ant-tooltip-container').forEach((el) => el.remove()));

  // —— 1. FAB「对话」打开浮窗 ——
  await page.getByRole('button', { name: '对话' }).click();
  const win = chatWindow(page);
  await expect(win).toBeVisible({ timeout: 8000 });

  // —— 2. 空态与输入提示 ——
  await expect(win).toContainText('还没有对话记录');
  await expect(win).toContainText('Enter 发送');
  // 工作空间/执行器选择
  await expect(win).toContainText('工作空间');
  await expect(win).toContainText('执行器');

  // —— 3. 输入框可输入（不发送） ——
  const input = win.locator('textarea:visible').first();
  await expect(input).toBeVisible();
  await input.fill('回归测试消息（不发送）');
  await expect(input).toHaveValue('回归测试消息（不发送）');
});
