// 工艺卡片点击体验验证（feat/process-card-click-detail）：
// 1. 点击卡片主体（标题/描述区）等同点击「详情」按钮 → 弹出详情 Modal；
// 2. 点击卡片底部 actions 区的按钮（安装/详情等）不触发卡片点击——
//    .ant-card-actions 守卫放行冒泡，避免「点安装却弹详情」互相遮挡。
//
// 运行前提：worktree make dev 已起（18088）。工艺列表与详情均 route mock，
// 不依赖 dev 库预置数据。

import { test, expect } from '@playwright/test';

const BASE = 'http://localhost:18088';

// mock 工艺列表（1 个用户工艺）+ 详情接口：列表有数据才能渲染卡片，
// 详情返回最小可解析载荷（Modal 渲染断言不依赖流程图解析成功）。
const PROCESS = {
  id: 1,
  guid: 'pw-card-click-guid',
  name: 'pw-card-click',
  display_name: '点击验证工艺',
  description: '用于验证卡片点击弹详情的工艺',
  category: 'software',
  complexity: 'light',
  version: '1.0.0',
  source_path: '~/.ntd/processes/pw-card-click.yaml',
  is_system: false,
  created_at: null,
  updated_at: null,
};

test.beforeEach(async ({ page }) => {
  await page.route('**/api/v1/bundled/processes**', (route) => {
    const url = route.request().url();
    // 列表接口（无 guid 路径段）返回数组；详情接口（含 guid 路径段）返回对象。
    if (/\/processes\/[^/]+$/.test(url)) {
      route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({
          code: 0,
          data: { ...PROCESS, definition: 'name: pw-card-click\nversion: 1.0.0\n' },
        }),
      });
    } else {
      route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({ code: 0, data: [PROCESS] }),
      });
    }
  });
});

/** 打开工艺页并等待卡片渲染。 */
async function openProcessPage(page: import('@playwright/test').Page) {
  await page.goto(`${BASE}/#/processes`, { waitUntil: 'domcontentloaded' });
  const card = page.locator('.ant-card').filter({ hasText: '点击验证工艺' }).first();
  await expect(card).toBeVisible({ timeout: 10000 });
  return card;
}

test('点击卡片主体弹出详情 Modal（等同点「详情」按钮）', async ({ page }) => {
  const card = await openProcessPage(page);

  // 点卡片标题区（非 actions 区）→ 详情 Modal 出现，含「流程图」Tab。
  await card.locator('.ant-card-head-title').click();
  const modal = page.locator('.ant-modal').filter({ hasText: '点击验证工艺' });
  await expect(modal).toBeVisible({ timeout: 8000 });
  await expect(modal.getByText('流程图', { exact: true }).first()).toBeVisible();
});

test('点击卡片描述区同样弹出详情 Modal', async ({ page }) => {
  const card = await openProcessPage(page);

  // 点正文描述（actions 区之外）→ 详情 Modal 出现。
  await card.locator('.ant-card-body').click();
  await expect(page.locator('.ant-modal').filter({ hasText: '点击验证工艺' })).toBeVisible({ timeout: 8000 });
});

test('点击卡片底部「安装」按钮：弹安装确认而非详情 Modal', async ({ page }) => {
  const card = await openProcessPage(page);

  // 点 actions 区「安装」：.ant-card-actions 守卫应放行按钮自身行为，
  // 只弹安装确认 Modal（「安装工艺模板」标题）；详情 Modal（含「流程图」Tab）不得出现。
  // 注意：安装确认文案含工艺名（将「点击验证工艺」安装到…），不能用工艺名区分 Modal，
  // 详情 Modal 以「流程图」Tab 为唯一特征。
  await card.getByRole('button', { name: '安装' }).click();
  const installModal = page.locator('.ant-modal').filter({ hasText: '安装工艺模板' });
  await expect(installModal).toBeVisible({ timeout: 8000 });
  const detailModal = page.locator('.ant-modal').filter({ hasText: '流程图' });
  await expect(detailModal).toHaveCount(0);
});

test('点击卡片底部「详情」按钮仍正常弹出详情 Modal', async ({ page }) => {
  const card = await openProcessPage(page);

  // actions 区「详情」按钮自身的行为不被守卫误伤。
  await card.getByRole('button', { name: '详情' }).click();
  await expect(page.locator('.ant-modal').filter({ hasText: '点击验证工艺' })).toBeVisible({ timeout: 8000 });
});
