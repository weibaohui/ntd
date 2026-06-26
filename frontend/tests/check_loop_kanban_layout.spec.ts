import { test, expect } from '@playwright/test';

/**
 * 验证环路视图的超宽内容只在卡片内部横向滚动。
 *
 * 这个用例专门防回归以下问题：
 * - 切到“环路视图”后，整页被看板列撑宽，导致页面主视图横向跳动
 * - 顶部四个视图切换按钮被整体挤出屏幕外
 * - 期望行为是页面主容器保持在视口内，只有看板列区域自己横向滚动
 */
test('环路视图仅在内容区横向滚动', async ({ page }) => {
  await page.goto('http://localhost:18088');
  await page.waitForLoadState('networkidle');

  const boardNav = page.locator('[aria-label="看板"]');
  await expect(boardNav).toBeVisible({ timeout: 5000 });
  await boardNav.click();

  const boardTitle = page.getByText('看板').first();
  await expect(boardTitle).toBeVisible({ timeout: 5000 });

  const loopKanbanOption = page.getByText('环路视图');
  await expect(loopKanbanOption).toBeVisible({ timeout: 5000 });
  await loopKanbanOption.click();

  await page.waitForLoadState('networkidle');

  const layoutMetrics = await page.evaluate(() => {
    const html = document.documentElement;
    const body = document.body;
    const pageCard = document.querySelector('.ntd-page-card') as HTMLElement | null;
    const pageCardExtra = document.querySelector('.ntd-page-card-extra') as HTMLElement | null;
    const columnsContainer = document.querySelector('.loop-kanban-columns-container') as HTMLElement | null;

    return {
      viewportWidth: window.innerWidth,
      htmlScrollWidth: html.scrollWidth,
      bodyScrollWidth: body.scrollWidth,
      pageCardScrollWidth: pageCard?.scrollWidth ?? 0,
      pageCardExtraRight: pageCardExtra?.getBoundingClientRect().right ?? 0,
      hasColumnsContainer: Boolean(columnsContainer),
      columnsClientWidth: columnsContainer?.clientWidth ?? 0,
      columnsScrollWidth: columnsContainer?.scrollWidth ?? 0,
      columnsRight: columnsContainer?.getBoundingClientRect().right ?? 0,
    };
  });

  expect(layoutMetrics.htmlScrollWidth).toBeLessThanOrEqual(layoutMetrics.viewportWidth + 2);
  expect(layoutMetrics.bodyScrollWidth).toBeLessThanOrEqual(layoutMetrics.viewportWidth + 2);
  expect(layoutMetrics.pageCardScrollWidth).toBeLessThanOrEqual(layoutMetrics.viewportWidth + 2);
  expect(layoutMetrics.pageCardExtraRight).toBeLessThanOrEqual(layoutMetrics.viewportWidth + 2);

  if (layoutMetrics.hasColumnsContainer) {
    expect(layoutMetrics.columnsRight).toBeLessThanOrEqual(layoutMetrics.viewportWidth + 2);
    expect(layoutMetrics.columnsScrollWidth).toBeGreaterThanOrEqual(layoutMetrics.columnsClientWidth);
  }
});
