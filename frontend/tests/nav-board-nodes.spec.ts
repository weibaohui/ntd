// 030-导航关系图黑板看板节点 Playwright 功能测试。
// 对应测试文档 docs/testing/030-导航关系图黑板看板节点-测试.md 的 TC-01 到 TC-06。
// baseURL 见 playwright.config.ts：http://localhost:5173（Vite dev）。
// 关系图为纯静态渲染，后端 API 不可达不影响本套件断言。

import { test, expect, type Page } from '@playwright/test';

/**
 * 每个用例前写入 onboarding 完成标记：
 * 防止「首登自动跳转 onboarding」逻辑（若存在）干扰 hash 断言，双保险。
 */
async function presetStorage(page: Page) {
  await page.addInitScript(() => {
    try { localStorage.setItem('ntd_onboarding_completed', 'true'); } catch { /* 静默 */ }
  });
}

/** 进入导航页并滚到关系图 section（各用例公共前置）。 */
async function gotoGraph(page: Page) {
  await page.goto('/#/onboarding');
  await page.getByRole('heading', { name: '概念关系图' }).scrollIntoViewIfNeeded();
}

/** 读取指定节点的 circle fill 属性（React 受控 attribute，hover 后同步更新）。 */
async function nodeFill(page: Page, nodeId: string): Promise<string | null> {
  return page
    .getByTestId(`onboarding-graph-node-${nodeId}`)
    .locator('circle')
    .evaluate((el) => el.getAttribute('fill'));
}

test.describe('030-导航关系图黑板看板节点', () => {
  test.beforeEach(async ({ page }) => {
    await presetStorage(page);
  });

  test('TC-01 关系图渲染 12 个节点，含黑板/看板', async ({ page }) => {
    await gotoGraph(page);
    await expect(page.getByTestId('onboarding-graph-node-blackboard')).toBeVisible();
    await expect(page.getByTestId('onboarding-graph-node-kanban')).toBeVisible();
    // 每节点 1 个 <circle>，marker 箭头用的是 <path>，圆总数应正好 12
    const circles = page.getByTestId('onboarding-relation-graph').locator('svg circle');
    await expect(circles).toHaveCount(12);
  });

  test('TC-02 点击黑板 → Drawer 三层语义说明 + 跳转黑板页', async ({ page }) => {
    await gotoGraph(page);
    await page.getByTestId('onboarding-graph-node-blackboard').click();
    // Drawer 文案须含「事项」「执行记录」「环节」三关键词（需求 §5.3 三层语义）
    const drawer = page.locator('.ant-drawer-open');
    await expect(drawer.getByText(/事项/)).toBeVisible();
    await expect(drawer.getByText(/执行记录/)).toBeVisible();
    await expect(drawer.getByText(/环节/)).toBeVisible();
    // 跳转按钮存在且文案正确
    const gotoBtn = page.getByTestId('onboarding-graph-drawer-goto-blackboard');
    await expect(gotoBtn).toBeVisible();
    await expect(gotoBtn).toHaveText('去黑板页');
    // 点击跳转：URL 到 /#/blackboard，Drawer 随视图卸载消失
    await gotoBtn.click();
    await expect(page).toHaveURL(/#\/blackboard/);
    await expect(page.locator('.ant-drawer-open')).toHaveCount(0);
  });

  test('TC-03 点击看板 → Drawer 说明 + 跳转看板页（kanban 模式）', async ({ page }) => {
    await gotoGraph(page);
    await page.getByTestId('onboarding-graph-node-kanban').click();
    const drawer = page.locator('.ant-drawer-open');
    // 需求 §5.4：必须表达「进度」语义
    await expect(drawer.getByText(/进度/)).toBeVisible();
    const gotoBtn = page.getByTestId('onboarding-graph-drawer-goto-kanban');
    await expect(gotoBtn).toBeVisible();
    await expect(gotoBtn).toHaveText('去看板页');
    await gotoBtn.click();
    // kanban 为唯一 query 参数，可直接全串匹配
    await expect(page).toHaveURL(/#\/memorial\?mode=kanban/);
  });

  test('TC-04 hover 黑板 → 事项/执行记录/环路同步高亮', async ({ page }) => {
    await gotoGraph(page);
    await page.getByTestId('onboarding-graph-node-blackboard').hover();
    // 等一帧让 React 状态 flush（fill 是受控 attribute，无 CSS 动画延迟）
    await page.waitForTimeout(300);
    expect(await nodeFill(page, 'todo')).toBe('#1677ff');
    expect(await nodeFill(page, 'execution')).toBe('#1677ff');
    expect(await nodeFill(page, 'loop')).toBe('#1677ff');
    // 无关节点（执行器）不应进入激活态
    expect(await nodeFill(page, 'executor')).not.toBe('#1677ff');
  });

  test('TC-05 hover 看板 → 执行记录高亮', async ({ page }) => {
    await gotoGraph(page);
    await page.getByTestId('onboarding-graph-node-kanban').hover();
    await page.waitForTimeout(300);
    expect(await nodeFill(page, 'execution')).toBe('#1677ff');
  });

  test('TC-06 回归：既有 fallback 节点（触发器）行为不变', async ({ page }) => {
    await gotoGraph(page);
    await page.getByTestId('onboarding-graph-node-trigger').click();
    const drawer = page.locator('.ant-drawer-open');
    // 通用文案逐字保留
    await expect(
      drawer.getByText('这是关系图的支线节点，用于辅助说明主概念关系，不对应独立菜单入口。'),
    ).toBeVisible();
    // 无任何跳转按钮
    await expect(drawer.locator('[data-testid^="onboarding-graph-drawer-goto-"]')).toHaveCount(0);
  });
});
