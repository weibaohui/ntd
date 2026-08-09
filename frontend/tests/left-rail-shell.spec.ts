import { test, expect } from '@playwright/test';

test('左侧主导航渲染并支持切换到设置', async ({ page }) => {
  await page.addInitScript(() => {
    localStorage.setItem('ntd_left_rail_collapsed', 'true');
  });
  await page.setViewportSize({ width: 1280, height: 720 });
  await page.goto('/');

  const rail = page.getByTestId('left-rail');
  await expect(rail).toBeVisible();
  await expect(page.getByTestId('left-rail-toggle')).toBeVisible();
  // 028 起 LeftRail 的 'items' 导航 key 已统一改名为 'todos'，
  // 收起态下不渲染任何 label span，这里校验 todos 项的 label 在收起时不存在。
  await expect(page.getByTestId('left-rail-label-todos')).toHaveCount(0);

  await page.getByTestId('left-rail-toggle').click();
  // 展开后 label span 重新渲染，todos 项 label 可见。
  await expect(page.getByTestId('left-rail-label-todos')).toBeVisible();

  await page.getByTestId('left-rail-workspace-switcher').click();
  await page.getByText('管理工作空间').click();
  // 管理工作空间页（ProjectDirectoriesPanel）的新建入口文案为「新建工作空间」，
  // 旧文案「添加项目目录」在重构后已不存在。
  await expect(page.getByText('新建工作空间').first()).toBeVisible();
});
