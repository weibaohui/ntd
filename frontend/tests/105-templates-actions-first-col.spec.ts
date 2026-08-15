// 模板管理 4 Tab 操作列布局验证（8-14 变更：操作列统一移到第一列 + 纯 icon 按钮化 +
// 来源说明 Alert 推广到专家/事项模板/技能 Tab）。
// 覆盖：
// 1. 专家模板 / 事项模板 / Skill 模板 / 工艺模板 四 Tab 表头第一列均为「操作」；
// 2. 工艺 Tab 操作区为纯 icon 按钮（无文字标签），分享按钮 iconOnly；
// 3. Skill 模板 Tab 顶部来源说明 Alert 渲染（技能全量可分享的说明文案）。
//
// 运行前提：make dev 已起（18088）。不点分享/删除等写操作，仅断言列结构与 Alert，
// 不依赖 PAT 配置，无需 route mock。

import { test, expect } from '@playwright/test';

const BASE = 'http://localhost:18088';

/** 进入设置-模板管理并等待面板渲染。 */
async function openTemplatesPanel(page: import('@playwright/test').Page) {
  await page.goto(`${BASE}/#/settings?tab=templates`, { waitUntil: 'domcontentloaded' });
  await expect(page.locator('.ntd-templates-panel')).toBeVisible({ timeout: 10000 });
  await page.waitForTimeout(800);
}

/** 切换到指定子 Tab 并断言表头第一列为「操作」。 */
async function expectFirstColumnIsAction(page: import('@playwright/test').Page, tabName: string) {
  await page.locator('.ntd-templates-panel .ant-tabs-tab').filter({ hasText: tabName }).first().click();
  await page.waitForTimeout(800);
  const firstTh = page.locator('.ant-table-thead th').first();
  await expect(firstTh).toHaveText('操作', { timeout: 8000 });
}

test('四 Tab 表头第一列均为「操作」', async ({ page }) => {
  await openTemplatesPanel(page);
  // 专家模板 / 事项模板 / Skill 模板 / 工艺模板逐一断言（操作列前置是 8-14 的全局调整）。
  await expectFirstColumnIsAction(page, '专家模板');
  await expectFirstColumnIsAction(page, '事项模板');
  await expectFirstColumnIsAction(page, 'Skill 模板');
  await expectFirstColumnIsAction(page, '工艺模板');
});

test('工艺模板操作区为纯 icon 按钮（无文字标签）', async ({ page }) => {
  // mock 工艺列表（固定 1 用户 + 1 系统，不依赖开发库预置）：
  // 分享按钮仅用户行渲染（与 027 同款来源守卫），断言时必须能定位到用户行。
  await page.route('**/api/v1/bundled/processes*', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        code: 0,
        data: [
          { id: 1, guid: 'pw-user-guid', name: 'pw-user-process', display_name: '我的工艺', description: '', category: 'software', complexity: 'light', version: '1.0.0', source_path: '~/.ntd/processes/pw-user-process.yaml', is_system: false, created_at: null, updated_at: null },
          { id: 2, guid: 'pw-sys-guid', name: 'pw-sys-process', display_name: '内置工艺', description: '', category: 'software', complexity: 'standard', version: '1.0.0', source_path: '~/.ntd/bundled/processes/software/pw-sys-process.yaml', is_system: true, created_at: null, updated_at: null },
        ],
      }),
    }),
  );

  await openTemplatesPanel(page);
  await page.locator('.ntd-templates-panel .ant-tabs-tab').filter({ hasText: '工艺模板' }).first().click();
  const processTab = page.locator('.process-templates-tab');
  await expect(processTab).toBeVisible({ timeout: 10000 });

  // 按来源 Tag 定位行（display_name 已避开「用户/系统」字样，行内文本唯一来自来源 Tag）。
  const rows = processTab.locator('tbody tr.ant-table-row');
  const userRow = rows.filter({ hasText: '用户' }).first();
  const systemRow = rows.filter({ hasText: '系统' }).first();

  // 用户行操作列：按钮无可见文字（iconOnly 化，8-14 变更），分享按钮以 share icon 定位。
  const userActionCell = userRow.locator('td').first();
  const btnTexts = await userActionCell.locator('button').allInnerTexts();
  for (const t of btnTexts) {
    expect(t.trim()).toBe('');
  }
  await expect(userActionCell.getByRole('button', { name: 'share' }).first()).toBeVisible();

  // 系统行：无分享按钮（来源守卫不回归）。
  await expect(systemRow.getByRole('button', { name: 'share' })).toHaveCount(0);
});

test('Skill 模板 Tab 显示来源说明 Alert', async ({ page }) => {
  await openTemplatesPanel(page);
  await page.locator('.ntd-templates-panel .ant-tabs-tab').filter({ hasText: 'Skill 模板' }).first().click();
  await page.waitForTimeout(800);

  // 来源说明 Alert：文案断言技能全量可分享（不区分来源）——8-14 推广到技能 Tab。
  const alert = page.locator('.skill-templates-tab .ant-alert').first();
  await expect(alert).toBeVisible({ timeout: 8000 });
  await expect(alert).toContainText('不区分来源');
});
