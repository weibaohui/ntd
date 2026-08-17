// check_tags_removed.spec.ts — 标签功能移除后的设置页冒烟验证。
//
// 验证点：
// 1. 设置页 Tabs 不再出现「标签管理」；
// 2. 旧 URL #/settings?tab=tags 平滑回退到「系统设置」Tab；
// 3. 页面不存在 TagsPanel 的特征文案（创建新标签 / 现有标签）。
import { test, expect } from '@playwright/test';

// 冒烟用 vite dev（5199），与正式 dev（18088 embedded）隔离，避免占用他人端口。
const BASE = 'http://localhost:5199';

test('设置页不再包含标签管理 Tab', async ({ page }) => {
  await page.goto(BASE + '/#/settings');
  await page.waitForSelector('.ant-tabs', { timeout: 20000 });

  // 左/顶部 Tabs 栏应出现「系统设置」，且整页不存在「标签管理」
  const tabsText = await page.locator('.ant-tabs').innerText();
  expect(tabsText).toContain('系统设置');
  expect(tabsText).not.toContain('标签管理');

  // TagsPanel 特征文案不应存在
  await expect(page.getByText('创建新标签')).toHaveCount(0);
  await expect(page.getByText('现有标签')).toHaveCount(0);
});

test('旧地址 ?tab=tags 平滑回退到系统设置', async ({ page }) => {
  await page.goto(BASE + '/#/settings?tab=tags');
  await page.waitForSelector('.ant-tabs', { timeout: 20000 });

  // 非法 tab 回退：激活项应为「系统设置」，而不是 404 或空面板
  const active = page.locator('.ant-tabs-tab-active');
  await expect(active).toContainText('系统设置');
  await expect(page.getByText('创建新标签')).toHaveCount(0);
});
