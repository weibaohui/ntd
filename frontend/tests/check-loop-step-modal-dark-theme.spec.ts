// 验证：Loop 编辑 Modal（LoopStudioStepsPanel）在暗色主题下颜色协调
// 1) 进入任意带 step 的 loop 详情
// 2) 点击流程图上的节点打开编辑 Modal
// 3) 切换到暗色主题并采样评分门禁/控制流/成功/失败四个关键区块的实际渲染颜色
// 4) 截图留档，断言暗色下背景与文字对比清晰、不再刺眼

import { test, expect } from '@playwright/test';
import { mkdirSync } from 'node:fs';
import { join } from 'node:path';

const BACKEND_URL = process.env.E2E_BACKEND_URL || 'http://localhost:18088';
const DEV_URL = 'http://localhost:18088';
const SCREENSHOT_DIR = 'test-results';

test('Loop 编辑 Modal 暗色主题颜色校验', async ({ page }) => {
  mkdirSync(SCREENSHOT_DIR, { recursive: true });

  // 找一个至少有 1 个 step 的 loop
  const listRes = await page.request.get(`${BACKEND_URL}/api/loops?page=1&limit=50`);
  const listJson = await listRes.json();
  const loops: Array<{ id: number; step_count: number }> = listJson?.data ?? [];
  const candidate = loops.find((l) => l.step_count > 0);
  test.skip(!candidate, '没有可用的带 step 的 loop，跳过用例');
  const loopId = candidate!.id;

  // 使用 system color scheme 强制暗色 + localStorage 同步暗色偏好
  await page.emulateMedia({ colorScheme: 'dark' });
  await page.addInitScript(() => {
    try { localStorage.setItem('app_theme', 'dark'); } catch {}
  });

  await page.goto(`${DEV_URL}/?loop=${loopId}`);
  await page.waitForLoadState('networkidle');
  await page.waitForTimeout(1200);

  // 验证 data-theme 已经设置成 dark
  const themeAttr = await page.evaluate(() => document.documentElement.getAttribute('data-theme'));
  expect(themeAttr, 'data-theme 应被设为 dark').toBe('dark');

  // 验证 --color-success-bg 已经切到深绿
  const successBgVar = await page.evaluate(() =>
    getComputedStyle(document.documentElement).getPropertyValue('--color-success-bg').trim(),
  );
  expect(successBgVar, '--color-success-bg 应为暗色 #052e16').toBe('#052e16');

  // 点击流程图上的节点
  const node = page.locator('svg [data-step-id], svg g[style*="pointer"]').first();
  if (await node.count() > 0) {
    await node.click({ force: true }).catch(() => {});
  } else {
    await page.locator('svg g').first().click({ force: true });
  }
  await page.waitForTimeout(800);

  // 等待 Modal 出现
  const modalTitle = page.getByText('编辑环节').first();
  await expect(modalTitle).toBeVisible({ timeout: 5000 });

  // 让 modal 撑满可视区，确保「成功时」「失败时」卡片都在 viewport 内可见
  await page.setViewportSize({ width: 1100, height: 1300 });
  await page.waitForTimeout(400);

  // 滚到 modal body 底部，让「评分不通过时」卡片可见
  const modalBody = page.locator('.ant-modal-body').last();
  if (await modalBody.count() > 0) {
    await modalBody.evaluate((el) => { el.scrollTop = el.scrollHeight; });
    await page.waitForTimeout(300);
  }

  await page.screenshot({ path: join(SCREENSHOT_DIR, `loop-step-modal-dark.png`), fullPage: false });

  // 采样「成功时」card 的实际 background-color
  const successBg = await page.evaluate(() => {
    const labels = Array.from(document.querySelectorAll('label'));
    const successLabel = labels.find((l) => l.textContent?.includes('成功时'));
    if (!successLabel) return null;
    let parent: HTMLElement | null = successLabel.parentElement;
    while (parent) {
      if (parent.style.background) {
        return getComputedStyle(parent).backgroundColor;
      }
      parent = parent.parentElement;
    }
    return null;
  });
  // 暗色 #052e16 → rgb(5, 46, 22)
  expect(successBg, '成功时卡片暗色背景应为深绿（rgb(5, 46, 22)）').toBe('rgb(5, 46, 22)');

  const failBg = await page.evaluate(() => {
    const labels = Array.from(document.querySelectorAll('label'));
    const failLabel = labels.find((l) => l.textContent?.includes('评分不通过时'));
    if (!failLabel) return null;
    let parent: HTMLElement | null = failLabel.parentElement;
    while (parent) {
      if (parent.style.background) {
        return getComputedStyle(parent).backgroundColor;
      }
      parent = parent.parentElement;
    }
    return null;
  });
  // 暗色 #450a0a → rgb(69, 10, 10)
  expect(failBg, '评分不通过时卡片暗色背景应为深红（rgb(69, 10, 10)）').toBe('rgb(69, 10, 10)');

  // 采样「评分门禁」section header 颜色
  const gateColor = await page.evaluate(() => {
    const headers = Array.from(document.querySelectorAll('div'));
    const gate = headers.find((d) => d.textContent?.trim() === '评分门禁');
    return gate ? getComputedStyle(gate).color : null;
  });
  // 暗色 #f9e2af → rgb(249, 226, 175)
  expect(gateColor, '评分门禁暗色前景应为柔和的浅黄（rgb(249, 226, 175)）').toBe('rgb(249, 226, 175)');
});
