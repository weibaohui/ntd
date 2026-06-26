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
  await expect(page.getByTestId('left-rail-label-inbox')).toHaveCount(0);

  await page.getByTestId('left-rail-toggle').click();
  await expect(page.getByTestId('left-rail-label-inbox')).toBeVisible();

  await page.getByTestId('left-rail-settings').click();
  await expect(page.getByText('系统设置')).toBeVisible();
});
