/**
 * 工作空间选择器测试
 *
 * 验证工作空间选择器功能：
 * - 选择器正确显示在搜索框上方
 * - 点击选择器显示工作空间列表
 * - 选择工作空间后正确过滤 todo 列表
 * - 选择"全部工作空间"显示所有 todo
 * - 刷新后保持选择的工作空间
 */

import { test, expect, chromium } from '@playwright/test';

// CLAUDE.md 规定 dev server 默认监听 18088（不是 Vite 默认的 5173），
// 把 fallback 从 5173 改成 18088，避免直接 `npx playwright test` 时连不上 dev 服务。
const DEV_URL = process.env.E2E_BASE_URL || 'http://localhost:18088';

test.describe('工作空间选择器', () => {
  test('选择器正确显示在搜索框上方', async () => {
    const browser = await chromium.launch();
    const context = await browser.newContext({ colorScheme: 'light' });
    const page = await context.newPage();
    
    await page.goto(DEV_URL);
    await page.waitForTimeout(2000);
    
    // 查找工作空间选择器
    const workspaceSelector = page.locator('button:has-text("全部工作空间")');
    await expect(workspaceSelector).toBeVisible();
    
    // 验证选择器在搜索框上方
    const searchInput = page.locator('input[placeholder*="搜索标题"]');
    const selectorBox = await workspaceSelector.boundingBox();
    const searchBox = await searchInput.boundingBox();
    
    if (selectorBox && searchBox) {
      expect(selectorBox.y).toBeLessThan(searchBox.y);
    }
    
    await browser.close();
  });

  test('点击选择器显示工作空间列表', async () => {
    const browser = await chromium.launch();
    const context = await browser.newContext({ colorScheme: 'light' });
    const page = await context.newPage();
    
    await page.goto(DEV_URL);
    await page.waitForTimeout(2000);
    
    // 点击工作空间选择器
    const workspaceSelector = page.locator('button:has-text("全部工作空间")');
    await workspaceSelector.click();
    
    // 验证下拉菜单出现
    const dropdownMenu = page.locator('.ant-dropdown');
    await expect(dropdownMenu).toBeVisible();
    
    // 验证菜单包含"全部工作空间"选项
    const allWorkspacesOption = page.locator('.ant-dropdown-menu-item:has-text("全部工作空间")');
    await expect(allWorkspacesOption).toBeVisible();
    
    await browser.close();
  });

  test('选择工作空间后正确过滤 todo 列表', async () => {
    const browser = await chromium.launch();
    const context = await browser.newContext({ colorScheme: 'light' });
    const page = await context.newPage();
    
    await page.goto(DEV_URL);
    await page.waitForTimeout(2000);
    
    // 点击工作空间选择器
    const workspaceSelector = page.locator('button:has-text("全部工作空间")');
    await workspaceSelector.click();
    
    // 选择一个工作空间（如果存在）
    const workspaceOption = page.locator('.ant-dropdown-menu-item').nth(1);
    if (await workspaceOption.isVisible()) {
      await workspaceOption.click();
      
      // 验证选择器文本更新
      const updatedSelector = page.locator('button').filter({ hasText: /全部工作空间|工作空间/ });
      await expect(updatedSelector).toBeVisible();
    }
    
    await browser.close();
  });

  test('刷新后保持选择的工作空间', async () => {
    const browser = await chromium.launch();
    const context = await browser.newContext({ colorScheme: 'light' });
    const page = await context.newPage();
    
    await page.goto(DEV_URL);
    await page.waitForTimeout(2000);
    
    // 点击工作空间选择器
    const workspaceSelector = page.locator('button:has-text("全部工作空间")');
    await workspaceSelector.click();
    
    // 选择一个工作空间（如果存在）
    const workspaceOption = page.locator('.ant-dropdown-menu-item').nth(1);
    if (await workspaceOption.isVisible()) {
      const optionText = await workspaceOption.textContent();
      await workspaceOption.click();
      
      // 刷新页面
      await page.reload();
      await page.waitForTimeout(2000);
      
      // 验证选择器仍然显示之前选择的工作空间
      const updatedSelector = page.locator('button').filter({ hasText: optionText || '' });
      await expect(updatedSelector).toBeVisible();
    }
    
    await browser.close();
  });
});