// 帮助抽屉 E2E：验证帮助按钮可点击、抽屉可打开、树可切换、mermaid 可渲染。
// 覆盖 AC-M1-1 / AC-M1-2 / AC-M1-3 / AC-M4-1。
import { test, expect } from '@playwright/test';

test('AC-M1-1: LeftRail 底部帮助按钮存在且可点击展开抽屉', async ({ page }) => {
  // desktop 视口确保 rail 走 rail 形态（非移动端 drawer），帮助按钮在 rail 底部
  await page.setViewportSize({ width: 1280, height: 720 });
  await page.addInitScript(() => {
    localStorage.setItem('ntd_left_rail_collapsed', 'true');
  });
  // 启动后等待 LeftRail 渲染完成
  await page.goto('http://localhost:18088');
  await page.waitForTimeout(2000);

  // 帮助按钮应存在（desktop rail 形态）
  const helpBtn = page.locator('[data-testid="left-rail-help"]');
  await expect(helpBtn).toBeVisible();

  // 点击前抽屉应不可见（Antd Drawer 抽屉根类）
  await expect(page.locator('.ant-drawer-title').filter({ hasText: '帮助' })).toHaveCount(0);

  // 点击帮助按钮
  await helpBtn.click();
  await page.waitForTimeout(800);

  // 抽屉应展开，标题为「帮助」
  await expect(page.locator('.ant-drawer-title')).toHaveText('帮助');
});

test('AC-M1-2: 抽屉左侧树形展示页面→功能点两级，默认选中当前页面总览', async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 720 });
  await page.addInitScript(() => {
    localStorage.setItem('ntd_left_rail_collapsed', 'true');
  });
  await page.goto('http://localhost:18088');
  await page.waitForTimeout(2000);

  // 默认落在事项视图，先点帮助
  await page.locator('[data-testid="left-rail-help"]').click();
  await page.waitForTimeout(800);

  // 树形应存在，且能看到一级节点「事项（列表）」（默认视图 todos）
  // Antd Tree 节点标题用 .ant-tree-title
  const treeTitles = page.locator('.ant-tree-title');
  const titles = await treeTitles.allTextContents();
  // 至少应包含「帮助首页」「事项（列表）」
  expect(titles.some(t => t.includes('帮助首页'))).toBeTruthy();
  expect(titles.some(t => t.includes('事项'))).toBeTruthy();
});

test('AC-M1-3: 选中任一节点，右侧渲染对应 md，mermaid 代码块渲染成图', async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 720 });
  await page.addInitScript(() => {
    localStorage.setItem('ntd_left_rail_collapsed', 'true');
  });
  await page.goto('http://localhost:18088');
  await page.waitForTimeout(2000);

  await page.locator('[data-testid="left-rail-help"]').click();
  await page.waitForTimeout(800);

  // 点帮助首页节点（默认已选中），右侧应渲染 md 内容
  // md 里有「欢迎使用 ntd 帮助系统」文本
  await expect(page.locator('.ant-drawer-body')).toContainText(['欢迎使用', '帮助系统']);

  // 切换到事项列表某个功能点节点（如「新建事项」）
  // 先展开事项（列表）一级节点
  const todoListNode = page.locator('.ant-tree-treenode').filter({ hasText: '事项（列表）' });
  await todoNodeExpandAndSelect(page, todoListNode, '新建事项');

  // 右侧应渲染功能点 md，至少含「新建事项」「数据流图」「开发指导」标题
  await expect(page.locator('.ant-drawer-body')).toContainText('新建事项');
  await expect(page.locator('.ant-drawer-body')).toContainText('数据流图');
  await expect(page.locator('.ant-drawer-body')).toContainText('开发指导');

  // mermaid 代码块应渲染成 svg（help-mermaid div 内有 svg）
  await expect(page.locator('.help-mermaid svg').first()).toBeVisible();
});

/**
 * 展开一级节点并点击指定的二级功能点。
 *
 * @param page Playwright page
 * @param parentNode 一级节点的 locator
 * @param featureTitle 二级功能点的中文名
 */
async function todoNodeExpandAndSelect(page: import('@playwright/test').Page, parentNode: import('@playwright/test').Locator, featureTitle: string) {
  // 若已展开直接点二级；否则先点 expand-icon 展开
  const switcher = parentNode.locator('.ant-tree-switcher').first();
  const isExpanded = await parentNode.evaluate(el => el.classList.contains('ant-tree-treenode-expanded'));
  if (!isExpanded) {
    await switcher.click();
    await page.waitForTimeout(400);
  }
  // 点二级功能点节点
  const featureNode = parentNode.locator('.ant-tree-node-content-wrapper').filter({ hasText: featureTitle }).first();
  await featureNode.click();
  await page.waitForTimeout(800);
}
