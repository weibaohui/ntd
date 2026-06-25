// 验证工作空间选择器已移除"全部工作空间"选项
import { test, expect } from '@playwright/test';

test('工作空间下拉菜单不应包含"全部工作空间"选项', async ({ page }) => {
  await page.goto('http://localhost:18088');
  await page.waitForTimeout(2000);

  // 查找工作空间选择按钮
  const workspaceButton = page.locator('button').filter({ hasText: /工作空间|全部工作空间|NTD/ }).first();

  // 点击打开工作空间下拉菜单
  await workspaceButton.click();
  await page.waitForTimeout(500);

  // 检查下拉菜单内容
  const menuText = await page.locator('.ant-dropdown-menu').textContent();
  console.log('下拉菜单内容:', menuText);

  // 验证不包含"全部工作空间"
  expect(menuText).not.toContain('全部工作空间');

  // 验证包含"管理工作空间"
  expect(menuText).toContain('管理工作空间');
});
