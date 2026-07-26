import { test, expect } from '@playwright/test';

const BASE = 'http://localhost:18088';

test.describe('Process Library 工艺模板库', () => {
  test('navigate to process page and render template cards', async ({ page }) => {
    await page.goto(`${BASE}/#/processes`);
    // 等待页面骨架渲染
    await page.waitForTimeout(1500);

    // 页面标题应存在
    const title = page.locator('text=工艺模板库');
    await expect(title).toBeVisible({ timeout: 5000 });

    // 刷新按钮应存在
    const refreshBtn = page.locator('button:has-text("刷新")');
    await expect(refreshBtn).toBeVisible({ timeout: 5000 });

    // 安装工作空间选择器应存在
    const wsSelect = page.locator('.ant-select', { hasText: '选择安装工作空间' });
    await expect(wsSelect).toBeVisible({ timeout: 5000 });

    // 如果已同步工艺模板，应能看到至少一个卡片
    const cards = page.locator('.ant-card');
    const count = await cards.count();
    if (count > 0) {
      // 首张卡片应包含详情/安装按钮
      await expect(page.locator('button:has-text("详情")').first()).toBeVisible();
      await expect(page.locator('button:has-text("安装")').first()).toBeVisible();
      console.log(`工艺模板库渲染了 ${count} 个模板卡片`);
    } else {
      console.log('暂无工艺模板卡片，可能尚未执行 bundled 同步');
    }
  });

  test('process API endpoints return valid structure', async ({ request }) => {
    const listResp = await request.get(`${BASE}/api/bundled/processes`);
    expect(listResp.ok()).toBeTruthy();
    const listBody = await listResp.json();
    expect(Array.isArray(listBody.data)).toBe(true);

    if (listBody.data.length > 0) {
      const first = listBody.data[0];
      expect(first.name).toBeDefined();
      expect(first.display_name).toBeDefined();

      const detailResp = await request.get(`${BASE}/api/bundled/processes/${encodeURIComponent(first.name)}`);
      expect(detailResp.ok()).toBeTruthy();
      const detailBody = await detailResp.json();
      expect(detailBody.data.definition).toBeDefined();
    }
  });
});
