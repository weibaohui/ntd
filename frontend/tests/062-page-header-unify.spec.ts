// 062-页面头部返回与标题统一：验证全部子页面返回按钮位置/样式/文案与标题格式。
//
// 设计文档：docs/design/062-页面头部返回与标题统一-设计.md
//
// 统一约定：
// 1. 返回按钮在页头 extra 区最右端（操作按钮之后），样式 ant-btn-sm + ant-btn-text + 左箭头；
// 2. 文案默认「返回列表」，目标非列表（帖子页→详情、Wiki→黑板）为「返回」；
// 3. 标题格式「模块名 #id: 具体名称」（无 id 的页面「模块名: 具体名称」）；
// 4. ProcessEditor 为自建 Toolbar，返回按钮同样位于操作区最右、样式对齐；
// 5. 移动端 MobileHeader 在 tasks 详情也显示返回按钮（此前遗漏）。
//
// 数据依赖：开发库（18088）需存在 todo#8 / loop#8 / task#32 / execution#46(属 todo#27) /
// wiki slug=code-quality-monitoring / 工艺 guid=4bafee67（E2E验证工艺055）。
// 缺数据时对应用例失败，请先核对开发库。

import { test, expect, type Page } from '@playwright/test';

const BASE = 'http://localhost:18088';

/** 等待 hash 路由生效 + 详情数据加载的统一延迟。 */
const ROUTE_SETTLE_MS = 1500;

/**
 * 断言 PageCard 头部：返回按钮位于 extra 区最右端，且为统一的 small+text 样式。
 * PageCard 将 onBack 按钮渲染为 .ntd-page-card-extra 的最后一个直接子元素。
 */
async function expectUnifiedBackButton(page: Page, label: string) {
  const backBtn = page.locator('.ntd-page-card-extra > button:last-child');
  await expect(backBtn).toBeVisible();
  await expect(backBtn).toContainText(label);
  // 统一样式：small + text（062 约定，参见 PageCard.onBack 实现）
  await expect(backBtn).toHaveClass(/ant-btn-sm/);
  await expect(backBtn).toHaveClass(/ant-btn-text/);
}

test.describe('062 页面头部返回与标题统一（桌面端）', () => {
  test('事项详情页：标题「事项 #id: 标题」+ 返回列表在 extra 最右端', async ({ page }) => {
    await page.goto(`${BASE}/#/todos/8`);
    await page.waitForTimeout(ROUTE_SETTLE_MS);

    // 标题格式：模块名 #id: 具体标题
    await expect(page.locator('.ntd-page-card-title-text').first()).toContainText('事项 #8: ');
    // 返回按钮在 extra 最右端（操作按钮之后），文案「返回列表」
    await expectUnifiedBackButton(page, '返回列表');
  });

  test('环路详情页：标题含环路名称 + 返回列表在 extra 最右端', async ({ page }) => {
    await page.goto(`${BASE}/#/loops/8`);
    await page.waitForTimeout(ROUTE_SETTLE_MS);

    // 062 新增：环路名称上提到 PageCard 标题（hideTitleRow 后内层不再展示名称）
    await expect(page.locator('.ntd-page-card-title-text').first()).toContainText('环路 #8: FEAT042-loop');
    await expectUnifiedBackButton(page, '返回列表');
  });

  test('任务详情页：标题「任务 #id: 标题」+ 返回列表在 extra 最右端', async ({ page }) => {
    await page.goto(`${BASE}/#/tasks/32`);
    await page.waitForTimeout(ROUTE_SETTLE_MS);

    await expect(page.locator('.ntd-page-card-title-text').first()).toContainText('任务 #32: ');
    await expectUnifiedBackButton(page, '返回列表');
  });

  test('帖子页：标题带事项前缀 + 返回（非返回列表）在 extra 最右端', async ({ page }) => {
    await page.goto(`${BASE}/#/todos/27/posts/46`);
    await page.waitForTimeout(ROUTE_SETTLE_MS);

    // 标题与事项详情页同格式（此前只有裸标题，丢了模块名与 id）
    await expect(page.locator('.ntd-page-card-title-text').first()).toContainText('事项 #27: ');
    // 返回目标是父级详情页而非列表，文案为「返回」
    await expectUnifiedBackButton(page, '返回');
  });

  test('Wiki 页：标题「Wiki: slug」+ antd 返回按钮（原生 button 已移除）', async ({ page }) => {
    await page.goto(`${BASE}/#/wiki?workspace=1&slug=code-quality-monitoring`);
    await page.waitForTimeout(ROUTE_SETTLE_MS);

    await expect(page.locator('.ntd-page-card-title-text').first()).toHaveText('Wiki: code-quality-monitoring');
    // 统一后返回按钮是 PageCard 渲染的 antd Button（不再是手写内联样式的原生 button）
    await expectUnifiedBackButton(page, '返回');
  });

  test('环路配置页：标题「环路配置: 工作空间名」+ 返回列表在 extra（不再占用 icon 位）', async ({ page }) => {
    // 配置页无独立 URL，从环路列表页「配置」按钮进入
    await page.goto(`${BASE}/#/loops`);
    await page.waitForTimeout(ROUTE_SETTLE_MS);
    await page.getByRole('button', { name: '配置' }).click();
    await page.waitForTimeout(ROUTE_SETTLE_MS);

    await expect(page.locator('.ntd-page-card-title-text').first()).toContainText('环路配置: ');
    await expectUnifiedBackButton(page, '返回列表');
  });

  test('工艺编辑器：标题「工艺: 显示名」+ 返回列表在操作区最右、样式对齐', async ({ page }) => {
    await page.goto(`${BASE}/#/processes?guid=4bafee67-a3e7-4c1f-b096-5f60ec8f6c14&processMode=edit`);
    await page.waitForTimeout(ROUTE_SETTLE_MS);

    // 标题格式统一（此前为「显示名 (唯一名)」，无模块名前缀）
    await expect(page.locator('text=工艺: E2E验证工艺055').first()).toBeVisible();

    // 返回按钮：操作区（antd Space）最后一个按钮，文案「返回列表」，样式 small+text
    const backBtn = page.getByRole('button', { name: '返回列表' });
    await expect(backBtn).toBeVisible();
    await expect(backBtn).toHaveClass(/ant-btn-sm/);
    await expect(backBtn).toHaveClass(/ant-btn-text/);
    // 位置断言：返回按钮是其父容器（操作区 Space）的最后一个按钮
    const isLast = await backBtn.evaluate((el) => {
      const parent = el.parentElement;
      if (!parent) return false;
      const buttons = parent.querySelectorAll('button');
      return buttons[buttons.length - 1] === el;
    });
    expect(isLast).toBe(true);
  });
});

test.describe('062 页面头部（移动端）', () => {
  // 移动端视口：触发 useIsMobile 分支，渲染 MobileHeader
  test.use({ viewport: { width: 375, height: 812 } });

  test('tasks 详情：MobileHeader 显示返回按钮（062 补齐）', async ({ page }) => {
    await page.goto(`${BASE}/#/tasks/32`);
    await page.waitForTimeout(ROUTE_SETTLE_MS);

    // 062 前 tasks 详情在移动端无返回入口；现与 todos/loops 一致
    await expect(page.locator('.mobile-header-menu-btn[aria-label="返回列表"]')).toBeVisible();
  });

  test('todos 详情：MobileHeader 返回按钮保持可用（回归）', async ({ page }) => {
    await page.goto(`${BASE}/#/todos/8`);
    await page.waitForTimeout(ROUTE_SETTLE_MS);

    await expect(page.locator('.mobile-header-menu-btn[aria-label="返回列表"]')).toBeVisible();
  });
});
