// 验证工作空间选择器已移除"全部工作空间"选项
import { test, expect } from '@playwright/test';

test('工作空间下拉菜单不应包含"全部工作空间"选项', async ({ page }) => {
  await page.goto('http://localhost:18088');
  // WorkspaceSwitcher 在 LeftRail 内渲染；桌面端 rail 默认折叠为 compact 模式，
  // 按钮仅图标无文本，旧版「按文本 工作空间 过滤 button」会匹配不到元素 → 30s 超时。
  // 改用 aria-label 定位：full 与 compact 两种模式均带 aria-label="切换工作空间"（见 WorkspaceSwitcher.tsx）。
  const workspaceButton = page.getByRole('button', { name: '切换工作空间' });
  await expect(workspaceButton).toBeVisible({ timeout: 15000 });

  // 点击打开工作空间下拉菜单
  await workspaceButton.click();
  // antd Dropdown 菜单挂载在 .ant-dropdown-menu；用 waitFor 取代固定 sleep，避免 CI flaky。
  const menu = page.locator('.ant-dropdown-menu').first();
  await expect(menu).toBeVisible({ timeout: 5000 });

  // 检查下拉菜单内容
  const menuText = (await menu.textContent()) ?? '';
  console.log('下拉菜单内容:', menuText);

  // 验证不包含"全部工作空间"（WorkspaceSwitcher 只列实际目录 + 新建/管理，从不注入此项）
  expect(menuText).not.toContain('全部工作空间');

  // 验证包含"管理工作空间"（LeftRail 注入了 onManage 回调，菜单底部渲染此项）
  expect(menuText).toContain('管理工作空间');
});
