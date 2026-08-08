/**
 * 帮助页同步（063/062/060/056）验证：
 * 通过左栏「帮助」按钮打开帮助弹窗，左侧菜单逐级点击，
 * 验证新增功能点页与更新过的页面均渲染正文、无 mermaid 语法错误。
 *
 * 菜单结构（HelpPage.tsx）：页面级 button.ntd-help-menu-item，
 * 功能点级 button.ntd-help-menu-sub-item；内容区 .ntd-help-main。
 */
import { test, expect } from '@playwright/test';

const BASE = 'http://localhost:18088';

/** 打开帮助弹窗。 */
async function openHelp(page: import('@playwright/test').Page) {
  await page.goto(BASE, { waitUntil: 'domcontentloaded' });
  await page.waitForTimeout(2000);
  await page.locator('button[data-testid="left-rail-help"]').first().click();
  await expect(page.locator('.ant-modal .ntd-help-menu').first()).toBeVisible({ timeout: 8000 });
  await page.waitForTimeout(500);
}

/** 点页面级节点（展开子菜单）再点功能点，验证内容区出现 marker。 */
async function clickFeatureAndExpect(
  page: import('@playwright/test').Page,
  pageTitle: string,
  featureTitle: string | null,
  marker: string,
) {
  await page.locator('.ntd-help-menu-item').filter({ hasText: pageTitle }).first().click();
  await page.waitForTimeout(600);
  if (featureTitle) {
    await page.locator('.ntd-help-menu-sub-item').filter({ hasText: featureTitle }).first().click();
    await page.waitForTimeout(800);
  }
  const content = page.locator('.ntd-help-main');
  await expect(content.locator(`text=${marker}`).first()).toBeVisible({ timeout: 8000 });
  // mermaid 语法错误会有显式报错文本，顺带拦截。
  await expect(content.locator('text=Syntax error in text')).toHaveCount(0);
}

test('063 新增「待审批透出与直达审批」功能点页可打开', async ({ page }) => {
  await openHelp(page);
  await clickFeatureAndExpect(page, '任务（列表）', '待审批透出与直达审批', '直达链路');
});

test('060 新增「讨论区」功能点页可打开', async ({ page }) => {
  await openHelp(page);
  await clickFeatureAndExpect(page, '任务（详情）', '讨论区', 'task_posts');
});

test('062 任务返回列表页含 extra 最右端口径', async ({ page }) => {
  await openHelp(page);
  await clickFeatureAndExpect(page, '任务（详情）', '返回列表', 'extra 区最右端');
});

test('056 事项列表页含服务端分页口径', async ({ page }) => {
  await openHelp(page);
  await clickFeatureAndExpect(page, '事项（列表）', null, '服务端分页');
});

test('056 事项搜索页含防抖下推服务端口径', async ({ page }) => {
  await openHelp(page);
  await clickFeatureAndExpect(page, '事项（列表）', '搜索过滤', '防抖');
});

test('导航页概念卡渲染（任务卡含待审批描述）', async ({ page }) => {
  await page.goto(`${BASE}/#/onboarding`, { waitUntil: 'domcontentloaded' });
  await page.waitForTimeout(2500);
  await expect(page.locator('text=任务').first()).toBeVisible({ timeout: 8000 });
  await expect(page.locator('text=待审批').first()).toBeVisible({ timeout: 8000 });
});
