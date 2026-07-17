import { test } from '@playwright/test';

const BASE = 'http://localhost:18088';

test('capture upgrade screenshots for PR', async ({ page }) => {
  // 设置窗口大小让截图清晰
  await page.setViewportSize({ width: 1440, height: 900 });

  await page.goto(`${BASE}/#/settings?tab=about`);
  await page.waitForTimeout(2500);

  // 触发 versionStatus
  const checkBtn = page.getByRole('button', { name: /检查更新/ }).first();
  await checkBtn.click();
  await page.waitForTimeout(2000);

  // 截图 1：关于页面 - 「手动升级」按钮区
  await page.screenshot({
    path: 'tests/__screenshots__/manual-upgrade-about.png',
    fullPage: true,
  });

  // 点击「手动升级」打开 Drawer
  const manualUpgradeBtn = page.getByRole('button', { name: /手动升级/ }).first();
  await manualUpgradeBtn.click();
  await page.waitForTimeout(800);

  // 截图 2：ActionButton Drawer - prompt 模板 + 执行器选择
  await page.screenshot({
    path: 'tests/__screenshots__/manual-upgrade-drawer.png',
    fullPage: true,
  });
});
