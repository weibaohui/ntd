import { test, expect } from '@playwright/test';

// 验证「设置 → 界面显示」tab 与底部执行日志面板显隐开关。
// 该开关为纯前端 UI 偏好（localStorage: ntd_console_panel_visible），
// 默认开启；切换后应即时生效并持久化。
const BASE = 'http://localhost:18088';
const STORAGE_KEY = 'ntd_console_panel_visible';

test('界面显示 tab 存在且开关默认开启', async ({ page }) => {
  // 先清掉 localStorage，确保读到默认值（默认开启）。
  await page.goto(BASE);
  await page.evaluate(() => localStorage.removeItem('ntd_console_panel_visible'));

  // 进入设置页（通过 URL 直接定位，避免依赖左侧导航的具体交互）。
  await page.goto(`${BASE}/#settings`);
  await page.waitForTimeout(800);

  // 点击「界面显示」tab。
  await page.getByRole('tab', { name: /界面显示/ }).click();
  await page.waitForTimeout(400);

  // 开关应存在且默认勾选。
  const sw = page.locator('.ant-switch').first();
  await expect(sw).toBeVisible();
  await expect(sw).toHaveClass(/ant-switch-checked/);
});

test('关闭开关后 localStorage 写入 false 且面板不渲染', async ({ page }) => {
  await page.goto(BASE);
  await page.evaluate(() => localStorage.setItem('ntd_console_panel_visible', 'true'));
  await page.reload();
  await page.goto(`${BASE}/#settings`);
  await page.waitForTimeout(800);
  await page.getByRole('tab', { name: /界面显示/ }).click();
  await page.waitForTimeout(400);

  const sw = page.locator('.ant-switch').first();
  await expect(sw).toBeVisible();

  // 关掉开关。
  await sw.click();
  await page.waitForTimeout(300);

  // localStorage 应已持久化为 false。
  const stored = await page.evaluate(() => localStorage.getItem('ntd_console_panel_visible'));
  expect(stored).toBe('false');

  // 关闭后底部执行日志面板不应渲染（即使无运行任务也本就不渲染，这里主要断言不存在）。
  await expect(page.locator('.execution-panel')).toHaveCount(0);
});
