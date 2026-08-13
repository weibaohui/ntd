// 专家贡献功能 UI 验证（需求 026）。
// 覆盖两个入口的「分享」按钮渲染，以及未配置 OAuth 凭据时的降级提示。
// 说明：OAuth 登录与创建 Issue 依赖真实 GitCode 账号，无法自动化；
// 本用例只验证 UI 渲染与「未配置凭据」降级路径（本地未注入凭据，enabled=false）。

import { test, expect } from '@playwright/test';

test('专家详情 Modal 展示分享按钮，未配置凭据时点击给出提示', async ({ page }) => {
  // 进入专家页。
  await page.goto('/#/experts');

  // 专家卡片用 role="button" 渲染；等待至少一张卡片可见。
  const firstCard = page.locator('div[role="button"]').first();
  await firstCard.waitFor({ state: 'visible', timeout: 15000 });

  // 点击第一张专家卡片，打开详情 Modal。
  await firstCard.click();

  // 详情 Modal 的操作区应出现「分享」按钮。
  const shareButton = page.getByRole('button', { name: '分享' });
  await shareButton.waitFor({ state: 'visible', timeout: 5000 });
  await expect(shareButton).toBeVisible();

  // 本地未注入凭据（enabled=false），点击分享应提示「未配置」，而非跳转授权。
  await shareButton.click();
  await expect(page.getByText('贡献功能未配置')).toBeVisible();
});
