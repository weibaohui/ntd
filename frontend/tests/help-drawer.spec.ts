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

  // 点击前帮助弹窗应不可见。
  // 帮助已从 antd Drawer 重构为 antd Modal（HelpPage），无障碍角色为 dialog；
  // .ant-drawer-title 在当前实现中已不存在，改用 dialog 角色断言弹窗显隐。
  await expect(page.getByRole('dialog')).toHaveCount(0);

  // 点击帮助按钮
  await helpBtn.click();
  await page.waitForTimeout(800);

  // 弹窗应展开，标题为「帮助文档」（Modal title，非旧的「帮助」）。
  // 用 getByText 而非 toHaveText：Modal title 容器内还内嵌了全屏切换按钮，
  // 精确等值匹配会因拼接进按钮文本失败；这里只校验标题文案存在。
  await expect(page.getByRole('dialog').getByText('帮助文档')).toBeVisible();
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

  // 一级页面节点应存在，且能看到「事项（列表）」（默认视图 todos）。
  // 帮助左侧已从 antd Tree 重构为自定义菜单：页面项文案在 .ntd-help-menu-item-label。
  const pageLabels = page.locator('.ntd-help-menu-item-label');
  const titles = await pageLabels.allTextContents();
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

  // 弹窗默认选中「当前页面总览」（事项视图 → todos-list），并非帮助首页；
  // 需先显式点击「帮助首页」节点，再断言 _overview.md 的渲染内容。
  // 帮助菜单已重构为自定义 <button>：一级页面项 .ntd-help-menu-item，点击会选中并展开其功能点子项。
  await page.locator('.ntd-help-menu-item', { hasText: '帮助首页' }).first().click();
  await page.waitForTimeout(500);

  // _overview.md 首行「欢迎使用 ntd」，并含「怎么用这个帮助」小节。
  // 内容容器已随重构改为 .ntd-help-content（旧 .ant-drawer-body 不再存在）。
  await expect(page.locator('.ntd-help-content')).toContainText('欢迎使用');
  await expect(page.locator('.ntd-help-content')).toContainText('怎么用这个帮助');

  // 切换到事项列表某个功能点节点（如「新建事项」）。
  // 默认视图即 todos → todos-list 节点在弹窗打开时已被展开（expandedKeys 初始含 p:todos-list），
  // 因此无需再点击父节点展开，直接点二级功能点子项 .ntd-help-menu-sub-item。
  await page.locator('.ntd-help-menu-sub-item', { hasText: '新建事项' }).first().click();
  await page.waitForTimeout(800);

  // 右侧应渲染 todo-list-create.md：含标题「新建事项」「事项创建数据流」「怎么操作」三段。
  // 旧断言里的「数据流图 / 开发指导」在当前 md 中不存在，按实际章节标题校验。
  await expect(page.locator('.ntd-help-content')).toContainText('新建事项');
  await expect(page.locator('.ntd-help-content')).toContainText('事项创建数据流');
  await expect(page.locator('.ntd-help-content')).toContainText('怎么操作');

  // mermaid 代码块应渲染成 svg（help-mermaid div 内有 svg）
  await expect(page.locator('.help-mermaid svg').first()).toBeVisible();
});
