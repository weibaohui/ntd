// 环路看板功能测试。
//
// 测试步骤：
// 1. 进入看板页面（通过头部导航）
// 2. 切换到环路看板视图
// 3. 验证工具栏出现（搜索框、时间过滤）
// 4. 验证视图切换成功

import { test, expect } from '@playwright/test';

test.describe('环路看板功能测试', () => {

  test.beforeEach(async ({ page }) => {
    // 访问首页
    await page.goto('http://localhost:18088');
    await page.waitForLoadState('networkidle');
    await page.waitForTimeout(1000);

    // 点击头部导航的"看板"入口，进入 MemorialBoard
    const memorialBtn = page.locator('[aria-label="看板"]');
    if (await memorialBtn.isVisible({ timeout: 3000 }).catch(() => false)) {
      await memorialBtn.click();
      await page.waitForTimeout(1000);
    }
  });

  test('看板页面视图切换选项包含四个选项', async ({ page }) => {
    // MemorialBoard 的 Segmented 在工具栏区域（第二个 Segmented）
    const segmented = page.locator('.ant-segmented').nth(1);
    await expect(segmented).toBeVisible({ timeout: 5000 });

    // 验证有四个视图选项：结论视图、看板视图、运行视图、环路看板
    const options = segmented.locator('.ant-segmented-item');
    await expect(options).toHaveCount(4);
  });

  test('切换到环路看板视图', async ({ page }) => {
    // 找到 MemorialBoard 的视图切换 Segmented
    const segmented = page.locator('.ant-segmented').nth(1);
    await segmented.waitFor({ timeout: 5000 });

    // 点击"环路看板"选项（第四个）
    const loopKanbanOption = segmented.locator('.ant-segmented-item').nth(3);
    await loopKanbanOption.click();

    // 等待视图切换
    await page.waitForTimeout(2000);

    // 验证环路看板的工具栏出现（搜索框）
    const searchInput = page.locator('input[placeholder*="环路"]');
    await expect(searchInput).toBeVisible({ timeout: 5000 });
  });

  test('环路看板显示看板组件', async ({ page }) => {
    // 先切换到环路看板
    const segmented = page.locator('.ant-segmented').nth(1);
    await segmented.waitFor({ timeout: 5000 });
    const loopKanbanOption = segmented.locator('.ant-segmented-item').nth(3);
    await loopKanbanOption.click();

    // 等待加载（可能需要更长时间因为要拉取 API）
    await page.waitForTimeout(3000);

    // 验证看板组件渲染（工具栏或列或空状态至少有一个可见）
    const toolbar = page.locator('.loop-kanban-toolbar');
    const columnHeaders = page.locator('.loop-kanban-column-header');
    const emptyState = page.locator('.ant-empty-description');
    const spin = page.locator('.ant-spin');

    // 至少工具栏应该可见（搜索框和时间选项）
    await expect(toolbar).toBeVisible({ timeout: 5000 });

    // 等待加载完成或超时（15秒内）
    let loaded = false;
    for (let i = 0; i < 15; i++) {
      const spinVisible = await spin.isVisible().catch(() => false);
      if (!spinVisible) {
        loaded = true;
        break;
      }
      await page.waitForTimeout(1000);
    }

    // 加载完成后，要么有列，要么有空状态
    if (loaded) {
      const hasColumns = await columnHeaders.count() > 0;
      const hasEmpty = await emptyState.isVisible().catch(() => false);
      // 至少有一种状态
      expect(hasColumns || hasEmpty).toBeTruthy();
    }
  });

  test('环路看板时间过滤功能', async ({ page }) => {
    // 先切换到环路看板
    const segmented = page.locator('.ant-segmented').nth(1);
    await segmented.waitFor({ timeout: 5000 });
    const loopKanbanOption = segmented.locator('.ant-segmented-item').nth(3);
    await loopKanbanOption.click();
    await page.waitForTimeout(2000);

    // 验证工具栏可见
    const toolbar = page.locator('.loop-kanban-toolbar');
    await expect(toolbar).toBeVisible({ timeout: 5000 });

    // 时间选项在工具栏内（第三个 Segmented）
    const timeOptions = page.locator('.ant-segmented').nth(2);
    if (await timeOptions.isVisible({ timeout: 2000 }).catch(() => false)) {
      const sevenDaysOption = timeOptions.locator('.ant-segmented-item').filter({ hasText: '7d' });
      await sevenDaysOption.click();
      await page.waitForTimeout(500);
    }
  });

  test('环路看板搜索功能', async ({ page }) => {
    // 先切换到环路看板
    const segmented = page.locator('.ant-segmented').nth(1);
    await segmented.waitFor({ timeout: 5000 });
    const loopKanbanOption = segmented.locator('.ant-segmented-item').nth(3);
    await loopKanbanOption.click();
    await page.waitForTimeout(2000);

    // 查找搜索框
    const searchInput = page.locator('input[placeholder*="环路"]');
    await expect(searchInput).toBeVisible({ timeout: 5000 });

    // 输入搜索内容
    await searchInput.fill('test');
    await page.waitForTimeout(500);

    // 清空搜索
    const clearButton = searchInput.locator('..').locator('.ant-input-clear-icon');
    if (await clearButton.isVisible({ timeout: 1000 }).catch(() => false)) {
      await clearButton.click();
    }
  });
});
