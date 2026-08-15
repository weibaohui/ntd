// 056：事项列表服务端分页验证（Playwright 临时调试脚本）
// 验证点：
// 1. 列表形态：分页控件展示总数、翻页后表格行变化
// 2. 卡片形态：bucket Tab 角标计数、底部 Pagination
// 3. 看板：brief 加载后卡片渲染
import { test, expect } from '@playwright/test';

const BASE = 'http://localhost:18088';

test.describe('056 服务端分页', () => {
  test('列表形态：分页控件与翻页', async ({ page }) => {
    await page.goto(`${BASE}/#/todos?view=list`);
    await page.waitForTimeout(2000);

    // 切到列表形态（antd 把 title 放在 .ant-segmented-item-label 上）
    const listBtn = page.locator('.ant-segmented-item-label[title="列表"]');
    if (await listBtn.count() > 0) {
      await listBtn.first().click();
      await page.waitForTimeout(1500);
    }

    // 分页控件应展示「共 N 项」且 N > 0（dev 库有种子数据）
    const totalText = page.locator('.ant-pagination-total-text');
    await expect(totalText).toBeVisible({ timeout: 5000 });
    const total = await totalText.textContent();
    console.log('列表分页总数:', total);

    // 表格应有行（dev 库有种子数据；空库环境下跳过本用例——
    // CodeRabbit#15：调试脚本依赖种子数据，干净环境不硬失败）
    const rows = page.locator('.ant-table-tbody tr.ant-table-row');
    const rowCount = await rows.count();
    console.log('当前页行数:', rowCount);
    test.skip(rowCount === 0, '当前环境无种子数据，跳过分页行数断言');

    // 行数应 ≤ 默认页大小 20（服务端分页的直接证据：不是全量）
    expect(rowCount).toBeLessThanOrEqual(20);

    // 翻页或页大小控件存在即视为分页生效（总数 ≤ 页大小时 antd 可能隐藏尺寸选择器）
    const pagination = page.locator('.ant-pagination');
    await expect(pagination).toBeVisible();

    await page.screenshot({ path: 'tests/__screenshots__/056-list-pagination.png' });
  });

  test('卡片形态：bucket 角标与底部分页', async ({ page }) => {
    await page.goto(`${BASE}/#/todos`);
    await page.waitForTimeout(2000);

    // 切到卡片形态（Segmented 为图标 + title 模式）
    const cardBtn = page.locator('.ant-segmented-item[title="卡片视图"], .ant-segmented-item[title="卡片"]');
    if (await cardBtn.count() > 0) {
      await cardBtn.first().click();
      await page.waitForTimeout(1500);
    }

    // bucket Tab 角标应显示数字（bucket_counts 后端聚合）
    const manualTab = page.locator('[data-testid="todo-center-tab-manual"]');
    await expect(manualTab).toBeVisible({ timeout: 5000 });
    const manualText = await manualTab.textContent();
    console.log('手动触发 Tab 角标:', manualText);

    // 底部应有 Pagination 控件
    const pagination = page.locator('.ant-pagination');
    await expect(pagination.first()).toBeVisible();
    const totalText = await pagination.first().locator('.ant-pagination-total-text').textContent();
    console.log('卡片分页总数:', totalText);

    await page.screenshot({ path: 'tests/__screenshots__/056-card-pagination.png' });
  });

  test('卡片视图：brief 加载渲染', async ({ page }) => {
    // 097/098 后 memorial?mode=kanban 已删，brief 卡片墙由事项页默认卡片视图承载
    // （viewMode='card' → TodoCenterCardView，卡片根节点 .todo-center-card）。
    await page.goto(`${BASE}/#/todos`);
    await page.waitForTimeout(2500);

    // 卡片视图是默认形态，无需切换。卡片或空态必居其一——两者都不可见才是失败
    // （白屏/loading 卡死），不能静默通过。
    const cards = page.locator('.todo-center-card');
    const empty = page.locator('.ant-empty');
    await expect(cards.or(empty).first()).toBeVisible({ timeout: 10000 });

    // 与本文件「列表形态」用例同款兜底：干净环境无种子数据时跳过 brief 抽查，
    // 避免环境敏感硬失败；dev 库有数据时 brief 链路是硬断言。
    const count = await cards.count();
    console.log('事项卡片数:', count);
    test.skip(count === 0, '当前环境无种子数据，跳过 brief 渲染断言');
    // brief 字段渲染抽查：卡片标题区应带 #id 前缀（TodoCenterCard 固定结构）。
    await expect(cards.first().locator('.todo-center-card-id')).toBeVisible();

    await page.screenshot({ path: 'tests/__screenshots__/056-card-brief.png' });
  });
});
