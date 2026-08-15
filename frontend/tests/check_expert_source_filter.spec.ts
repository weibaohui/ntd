// 专家页来源筛选验证：全部 / 我的 / 内置。
// 与 026 分享限制配套——「我的」=用户自定义专家（可分享、可编辑），
// 「内置」=从官方仓库同步的系统专家（只读）。
//
// 数据策略：route mock 注入确定性专家列表（1 用户 + 1 系统），不依赖
// ~/.ntd/experts/ 的环境数据——该目录在干净环境不存在，真实后端只有系统专家，
// 曾导致本用例 15s 超时（用户卡片永远不出现）。范式同 092-create-task-delegate。

import { test, expect } from '@playwright/test';
// 专家 mock 载荷构造收敛到共享 helper（与 026 分享用例同源，防两处漂移）
import { mockExpert, mockExpertsResponse } from './helpers/expertMock';

const BASE = 'http://localhost:18088';

test.beforeEach(async ({ page }) => {
  // 拦截专家列表接口：注入固定 1 用户 + 1 系统专家，断言不依赖环境数据量。
  await page.route('**/api/v1/experts', (route) =>
    route.fulfill(mockExpertsResponse([
      mockExpert('mock-user-expert', 'user'),
      mockExpert('mock-system-expert', 'system'),
    ])),
  );
});

test('专家页来源筛选：全部默认可见，「我的/内置」互斥过滤且可切回', async ({ page }) => {
  await page.goto(`${BASE}/#/experts`);

  // 等专家卡片（role=button）加载完成。
  const userCard = page.locator('div[role="button"][data-source="user"]').first();
  await userCard.waitFor({ state: 'visible', timeout: 15000 });

  // 默认「全部」：用户与系统卡片同时可见。
  await expect(page.locator('div[role="button"][data-source="system"]').first()).toBeVisible();

  // 切「我的」：只剩用户卡片，系统卡片全部消失。
  await page.locator('.ant-segmented-item').filter({ hasText: '我的' }).click();
  await expect(page.locator('div[role="button"][data-source="system"]')).toHaveCount(0);
  await expect(page.locator('div[role="button"][data-source="user"]').first()).toBeVisible();

  // 切「内置」：只剩系统卡片，用户卡片全部消失。
  await page.locator('.ant-segmented-item').filter({ hasText: '内置' }).click();
  await expect(page.locator('div[role="button"][data-source="user"]')).toHaveCount(0);
  await expect(page.locator('div[role="button"][data-source="system"]').first()).toBeVisible();

  // 切回「全部」：两类卡片都恢复可见。
  await page.locator('.ant-segmented-item').filter({ hasText: '全部' }).click();
  await expect(page.locator('div[role="button"][data-source="system"]').first()).toBeVisible();
  await expect(page.locator('div[role="button"][data-source="user"]').first()).toBeVisible();
});
