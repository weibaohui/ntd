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

  // 抽屉默认选中「当前页面总览」（事项视图 → todos-list），并非帮助首页；
  // 需先显式点击「帮助首页」节点，再断言 _overview.md 的渲染内容。
  // 原断言依赖「默认选中帮助首页」的错误假设，叠加 NTD-011 内容空白 bug，该用例自 PR #972 起从未通过。
  // 定位器统一限定 .ant-drawer-open 作用域：页面若同时存在其他 Tree/Drawer，无作用域选择器可能误点（PR #978 评审）。
  await page.locator('.ant-drawer-open .ant-tree-title').filter({ hasText: '帮助首页' }).first().click();
  await page.waitForTimeout(500);

  // md 里有「欢迎使用 ntd 帮助系统」文本。
  // 注意必须拆成两条断言：toContainText 传数组时 Playwright 会要求 locator 解析为等长的元素列表，
  // 而 .ant-drawer-body 只有一个元素，数组写法（原写法）在任何情况下都必然失败。
  await expect(page.locator('.ant-drawer-open .ant-drawer-body')).toContainText('欢迎使用');
  await expect(page.locator('.ant-drawer-open .ant-drawer-body')).toContainText('帮助系统');

  // 切换到事项列表某个功能点节点（如「新建事项」）
  // 先展开事项（列表）一级节点
  const todoListNode = page.locator('.ant-drawer-open .ant-tree-treenode').filter({ hasText: '事项（列表）' });
  await todoNodeExpandAndSelect(page, todoListNode, '新建事项');

  // 右侧应渲染功能点 md，至少含「新建事项」「数据流图」「开发指导」标题
  await expect(page.locator('.ant-drawer-open .ant-drawer-body')).toContainText('新建事项');
  await expect(page.locator('.ant-drawer-open .ant-drawer-body')).toContainText('数据流图');
  await expect(page.locator('.ant-drawer-open .ant-drawer-body')).toContainText('开发指导');

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
  // 子节点标题在树中可见，说明父节点已展开，直接点二级。
  const featureTitleLocator = page.locator('.ant-drawer-open .ant-tree-title').filter({ hasText: featureTitle }).first();
  // 未展开时点击父节点「标题文字」展开：NTD-011 修复后点标题 = 选中 + 展开，且不会反向收起。
  // 不点 switcher 箭头的原因：箭头是「切换」语义，若节点已展开会被误收起；
  // 且不能依赖 ant-tree-treenode-expanded 等内部 class 判断展开态（antd 版本间可能更名）。
  if (!(await featureTitleLocator.isVisible())) {
    await parentNode.locator('.ant-tree-title').first().click();
    await page.waitForTimeout(400);
  }
  // 点二级功能点节点。
  // 注意：antd Tree 的节点是平级渲染，子节点并不嵌套在父节点 treenode 的 DOM 内，
  // 因此必须在整个抽屉范围内按标题查找，不能在 parentNode 内部 locator（原写法永远找不到）。
  await featureTitleLocator.click();
  await page.waitForTimeout(800);
}
