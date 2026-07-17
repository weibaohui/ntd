import { test, expect } from '@playwright/test';

// 「设置 → 关于 → 手动升级」改造成 ActionButton 的回归测试。
// 背景：旧版是一个 Collapse 折叠面板，让用户复制命令到 AI 工具中手动执行；
// 新版直接用 ActionButton 让 AI 在执行器里跑升级命令，免去复制步骤。

const BASE = 'http://localhost:18088';

test('未检查更新前不展示「手动升级」按钮', async ({ page }) => {
  await page.goto(`${BASE}/#/settings?tab=about`);
  // 等 AboutPanel 挂载完成，但不要点击「检查更新」
  await page.waitForTimeout(1500);

  // versionStatus === null 时，不应渲染「手动升级」按钮
  await expect(page.getByRole('button', { name: /手动升级/ })).toHaveCount(0);

  // 同时旧的「复制命令」相关文案也应不再出现
  await expect(page.getByText(/复制命令到 AI 工具中执行/)).toHaveCount(0);

  // ActionButton 自身渲染的「AI 一键执行」提示文案
  await expect(page.getByText(/一键执行/)).toHaveCount(0);
});

test('检查更新后展示「手动升级」按钮（ActionButton 形态）', async ({ page }) => {
  await page.goto(`${BASE}/#/settings?tab=about`);
  // 等 AboutPanel 挂载完成 + /api/version 响应回来，禁用「检查更新」按钮才会打开
  await page.waitForTimeout(2500);

  // 1) 点击「检查更新」按钮触发 versionStatus
  const checkBtn = page.getByRole('button', { name: /检查更新/ }).first();
  await expect(checkBtn).toBeEnabled({ timeout: 10000 });
  await checkBtn.click();

  // 2) 等待 getLatestVersion 响应 + 渲染 manual upgrade 区域
  await page.waitForTimeout(2000);

  // 3) ActionButton 渲染的「手动升级」按钮应可见
  const manualUpgradeBtn = page.getByRole('button', { name: /手动升级/ }).first();
  await expect(manualUpgradeBtn).toBeVisible();
});

test('点击「手动升级」按钮打开 Drawer 并看到执行器选择器', async ({ page }) => {
  await page.goto(`${BASE}/#/settings?tab=about`);
  await page.waitForTimeout(2500);

  // 触发 versionStatus
  const checkBtn = page.getByRole('button', { name: /检查更新/ }).first();
  await expect(checkBtn).toBeEnabled({ timeout: 10000 });
  await checkBtn.click();
  await page.waitForTimeout(2000);

  // 点击「手动升级」按钮
  const manualUpgradeBtn = page.getByRole('button', { name: /手动升级/ }).first();
  await manualUpgradeBtn.click();

  // Drawer 标题「手动升级（AI 一键执行）」应出现
  await expect(page.getByText(/手动升级（AI 一键执行）/)).toBeVisible();

  // Drawer 内有「执行」按钮（ActionButton 的 idle 态）
  // 按钮内文字含图标+空格，aria name 不可靠；用 text 选择器兜底
  await expect(page.getByText(/^执\s*行$/).first()).toBeVisible();

  // Drawer 内有「取消」按钮
  await expect(page.getByText(/^取\s*消$/).first()).toBeVisible();
});
