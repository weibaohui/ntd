import { test, expect } from '@playwright/test';

/**
 * 验证执行器页面改造：
 * 1. 执行器页面有三个 tab：执行器 / 正在运行 / 会话
 * 2. 左侧菜单已移除"运行管理"和"会话"菜单项
 */
test('执行器页面有执行器/正在运行/会话三个tab', async ({ page }) => {
  await page.goto('http://localhost:18088');
  await page.waitForLoadState('networkidle');

  // 点击左侧"执行器"菜单
  await page.click('[data-testid="left-rail-settings_executors"]');
  await page.waitForTimeout(800);

  // 验证有三个 tab
  const tabExecutors = page.locator('.ant-tabs-tab').filter({ hasText: '执行器' });
  const tabRunning = page.locator('.ant-tabs-tab').filter({ hasText: '正在运行' });
  const tabSessions = page.locator('.ant-tabs-tab').filter({ hasText: '会话' });

  await expect(tabExecutors).toBeVisible();
  await expect(tabRunning).toBeVisible();
  await expect(tabSessions).toBeVisible();

  // 点击"正在运行" tab
  await tabRunning.click();
  await page.waitForTimeout(500);
  // antd v6 不卸载非激活 tab，会话 tab 的「刷新」按钮仍留在 DOM 中，
  // 全局 text=刷新 会命中 4 个节点（2 个按钮 × button+内层 span）触发 strict-mode 误报。
  // 限定到 .ant-tabs-tabpane-active（当前即「正在运行」面板），只匹配本 tab 的工具条按钮。
  // 批量停止 同理用正则匹配（按钮文案是动态的「批量停止 (N)」）。
  await expect(page.locator('.ant-tabs-tabpane-active').getByRole('button', { name: '刷新' })).toBeVisible();
  await expect(page.locator('.ant-tabs-tabpane-active').getByRole('button', { name: /批量停止/ })).toBeVisible();

  // 点击"会话" tab
  await tabSessions.click();
  await page.waitForTimeout(500);
  // 会话 tab 有 StatsCards 和搜索框
  await expect(page.locator('input[placeholder*="搜索"]')).toBeVisible();

  console.log('✓ 执行器页面 Tab 切换正常');
});

test('左侧菜单已移除运行管理和会话', async ({ page }) => {
  await page.goto('http://localhost:18088');
  await page.waitForLoadState('networkidle');

  // 展开左侧菜单
  const toggleBtn = page.locator('[data-testid="left-rail-toggle"]');
  if (await toggleBtn.isVisible()) {
    await toggleBtn.click();
    await page.waitForTimeout(300);
  }

  // 确认"运行管理"菜单项不存在
  const runtimeMenu = page.locator('[data-testid="left-rail-settings_runtime"]');
  await expect(runtimeMenu).toHaveCount(0);

  // 确认"会话"菜单项不存在
  const sessionsMenu = page.locator('[data-testid="left-rail-settings_sessions"]');
  await expect(sessionsMenu).toHaveCount(0);

  // 确认"执行器"菜单项仍然存在
  const executorMenu = page.locator('[data-testid="left-rail-settings_executors"]');
  await expect(executorMenu).toBeVisible();

  console.log('✓ 运行管理和会话菜单已移除，执行器菜单保留');
});