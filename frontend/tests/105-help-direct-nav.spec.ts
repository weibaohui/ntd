// 帮助系统直达当前视图验证（需求 #972 系列：帮助按钮直接打开当前视图对应的具体帮助页 +
// 路由化 #/help/<pageId>/<featureId> + 内嵌大弹窗 + 暗色主题无白边）。
// 覆盖：
// 1. 列表/详情双形态视图打开帮助时，菜单选中项与 viewToPageId 映射一致（todos-list/todos-detail）；
// 2. 单形态视图（blackboard/messages）直达对应页面；
// 3. 菜单导航同页跳转：点其他页面节点后选中态切换、内容区渲染对应概览；
// 4. 暗色主题下弹窗内容区无白色背景残留（08-03 修复项回归）。
//
// 运行前提：make dev 已起（18088）。不依赖后端造数：详情页即使无数据，
// activeView/detail 形态已决定 viewToPageId 输出。

import { test, expect, chromium } from '@playwright/test';

const BASE = 'http://localhost:18088';

/** 打开帮助弹窗并等待菜单渲染。 */
async function openHelp(page: import('@playwright/test').Page) {
  await page.locator('button[data-testid="left-rail-help"]').first().click();
  await expect(page.locator('.ant-modal .ntd-help-menu').first()).toBeVisible({ timeout: 8000 });
  await page.waitForTimeout(400);
}

/** 断言弹窗菜单当前选中项（.is-selected）的文本包含预期标题。 */
async function expectSelectedPage(page: import('@playwright/test').Page, title: string) {
  const selected = page.locator('.ntd-help-menu-item.is-selected').first();
  await expect(selected).toBeVisible({ timeout: 5000 });
  await expect(selected).toContainText(title);
}

test('事项列表页打开帮助：选中「事项（列表）」', async ({ page }) => {
  await page.goto(`${BASE}/#/todos`, { waitUntil: 'domcontentloaded' });
  await page.waitForTimeout(1500);
  await openHelp(page);
  // viewToPageId(todos, 无详情) === 'todos-list' → 菜单标题「事项（列表）」
  await expectSelectedPage(page, '事项（列表）');
});

test('事项详情页打开帮助：选中「事项（详情）」', async ({ page }) => {
  // 直接用详情路由 #/todos/39（060 spec 同款既有数据 id）；即使行不存在，
  // 详情形态的 viewToPageId 输出仍为 todos-detail。
  await page.goto(`${BASE}/#/todos/39`, { waitUntil: 'domcontentloaded' });
  await page.waitForTimeout(1500);
  await openHelp(page);
  await expectSelectedPage(page, '事项（详情）');
});

test('黑板页打开帮助：选中「黑板」', async ({ page }) => {
  await page.goto(`${BASE}/#/blackboard`, { waitUntil: 'domcontentloaded' });
  await page.waitForTimeout(1500);
  await openHelp(page);
  await expectSelectedPage(page, '黑板');
});

test('消息页打开帮助：选中「消息」', async ({ page }) => {
  await page.goto(`${BASE}/#/messages`, { waitUntil: 'domcontentloaded' });
  await page.waitForTimeout(1500);
  await openHelp(page);
  await expectSelectedPage(page, '消息');
});

test('菜单导航同页跳转：点「黑板」节点后选中态切换且内容区渲染', async ({ page }) => {
  await page.goto(`${BASE}/#/todos`, { waitUntil: 'domcontentloaded' });
  await page.waitForTimeout(1500);
  await openHelp(page);

  // 点黑板页面节点：同页跳转，不新开窗口（帮助是内嵌大弹窗）。
  await page.locator('.ntd-help-menu-item').filter({ hasText: '黑板' }).first().click();
  await page.waitForTimeout(500);
  await expectSelectedPage(page, '黑板');
  // 内容区渲染黑板的概览文档（非空即视为已切换渲染）。
  const main = page.locator('.ntd-help-main');
  await expect(main).toBeVisible();
  const text = (await main.innerText()).trim();
  expect(text.length).toBeGreaterThan(0);
  // mermaid 语法错误会有显式报错文本，顺带拦截。
  await expect(main.locator('text=Syntax error in text')).toHaveCount(0);
});

test('暗色主题打开帮助：内容区无白色背景残留', async () => {
  // 独立 browser context 预设 colorScheme=dark：让主题在初始化阶段直接进入暗色分支，
  // 与 check_theme 系列同一手法。
  const browser = await chromium.launch();
  const context = await browser.newContext({ colorScheme: 'dark' });
  const page = await context.newPage();
  await page.goto(`${BASE}/#/todos`, { waitUntil: 'domcontentloaded' });
  await page.waitForTimeout(1500);
  await openHelp(page);

  // 采集 .ntd-help-page 的实际背景色：暗色主题下应为深色（R/G/B 均 < 128），
  // 出现纯白/接近白（如 255,255,255）即 08-03「暗色白边」回潮。
  const bg = await page
    .locator('.ntd-help-page')
    .first()
    .evaluate((el) => {
      const c = getComputedStyle(el).backgroundColor;
      const m = c.match(/rgba?\((\d+),\s*(\d+),\s*(\d+)/);
      return m ? { r: +m[1], g: +m[2], b: +m[3] } : null;
    });
  expect(bg).not.toBeNull();
  expect(bg!.r).toBeLessThan(128);
  expect(bg!.g).toBeLessThan(128);
  expect(bg!.b).toBeLessThan(128);

  await browser.close();
});
