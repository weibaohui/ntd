import { test, expect } from '@playwright/test';
// antd 图标式 Segmented 选项定位收敛到共享 helper（DOM 细节单点维护）
import { segmentedOption } from './helpers/segmented';

/**
 * 验证环路视图的超宽内容只在卡片内部横向滚动。
 *
 * 这个用例专门防回归以下问题：
 * - 切到“环路视图”后，整页被看板列撑宽，导致页面主视图横向跳动
 * - 顶部四个视图切换按钮被整体挤出屏幕外
 * - 期望行为是页面主容器保持在视口内，只有看板列区域自己横向滚动
 */
test('环路视图仅在内容区横向滚动', async ({ page }) => {
  // 097/098 后入口变迁：环路看板归位 /#/loops 页内 Segmented（原「侧栏看板 → 环路视图」
  // 四视图链路已删），直达 hash 路由再切看板态。
  await page.goto('http://localhost:18088/#/loops');
  await page.waitForLoadState('networkidle');

  // 等视图切换器可见作为「页面已就绪」信号
  const toggle = page.getByTestId('loop-list-view-toggle');
  await expect(toggle).toBeVisible({ timeout: 5000 });

  // 切看板态：antd 图标式选项定位细节（title 在 label div 上、radio 视觉隐藏）
  // 收敛在 helper 内，本处只表达「点看板选项」的意图。
  await segmentedOption(page, '看板', toggle).click();
  await expect(page.locator('.loop-kanban-board')).toBeVisible({ timeout: 5000 });

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
