// 033-collapse-expand-click.spec.ts
// ---------------------------------------------------------------------------
// 验证收起态窄条的「点箭头」与「点竖条」都能重新展开属性面板。
// 对应修复：内部箭头/标题设 pointer-events:none，所有点击归到外层 button。
// ---------------------------------------------------------------------------
import { test, expect } from '@playwright/test';

const BASE = process.env.UI_BASE || 'http://localhost:18088';
const EDIT_URL = `${BASE}/#/processes?processMode=edit&name=4p12s-delivery`;

test.describe('033 收起态点击展开', () => {
  test('点箭头可展开属性面板', async ({ page }) => {
    await page.goto(EDIT_URL);
    await page.waitForTimeout(3000);
    await page.locator('.react-flow__node-link').first().click({ timeout: 5000, force: true });
    await page.waitForTimeout(1200);

    // 收起
    await page.locator('button[aria-label="收起属性面板"]').click();
    await page.waitForTimeout(600);
    const expandBtn = page.locator('button[aria-label="展开属性面板"]');
    await expect(expandBtn).toBeVisible();

    // 点击箭头（svg）应展开（真实点击，不再依赖 force）
    await expandBtn.locator('svg').first().click();
    await page.waitForTimeout(600);
    await expect(page.locator('button[aria-label="收起属性面板"]')).toBeVisible();
  });

  test('点竖条（标题区）可展开属性面板', async ({ page }) => {
    await page.goto(EDIT_URL);
    await page.waitForTimeout(3000);
    await page.locator('.react-flow__node-link').first().click({ timeout: 5000, force: true });
    await page.waitForTimeout(1200);

    await page.locator('button[aria-label="收起属性面板"]').click();
    await page.waitForTimeout(600);
    const expandBtn = page.locator('button[aria-label="展开属性面板"]');
    await expect(expandBtn).toBeVisible();

    // 点击竖条标题文字应展开
    await expandBtn.click();
    await page.waitForTimeout(600);
    await expect(page.locator('button[aria-label="收起属性面板"]')).toBeVisible();
  });
});
