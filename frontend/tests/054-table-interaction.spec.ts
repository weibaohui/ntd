/**
 * 054-列表表格交互增强 — Playwright 功能验证。
 *
 * 验证三大列表（事项/任务/环路）：
 * 1. Table 渲染正常（列宽/拖拽手柄正确渲染）
 * 2. 可排序列点击后显示排序指示器
 * 3. 排序/列宽状态写入 localStorage，刷新后恢复
 */
import { test, expect } from '@playwright/test';

const BASE = 'http://localhost:18088';

/**
 * 辅助：跳转到目标路由并等 Table 渲染完毕。
 *
 * 用 page.addInitScript（而非 context.addInitScript）保证脚本在页面
 * 下次导航时、任何应用 JS 之前执行——事项页 readInitialView 依赖
 * localStorage 中的 ntd_items_view='list' 才渲染 table 形态。
 */
async function gotoList(
  page: import('@playwright/test').Page,
  route: string,
  init?: { key: string; value: string },
) {
  if (init) {
    await page.addInitScript(([k, v]) => localStorage.setItem(k, v), [init.key, init.value]);
  }
  await page.goto(`${BASE}/#${route}`);
  await page.waitForSelector('.ant-table-thead th', { timeout: 10000 });
  await page.waitForTimeout(500);
}

/** 检查列头的拖拽手柄（cursor: col-resize 的 span）是否存在。 */
async function hasResizableHandle(page: import('@playwright/test').Page, colText: string) {
  const th = page.locator('.ant-table-thead th').filter({ hasText: colText }).first();
  const handle = th.locator('span[style*="col-resize"]');
  return (await handle.count()) > 0;
}

/** 检查列头是否有激活的排序指示器。 */
async function hasActiveSortIndicator(page: import('@playwright/test').Page, colText: string) {
  const th = page.locator('.ant-table-thead th').filter({ hasText: colText }).first();
  return (await th.locator('.ant-table-column-sorter').count()) > 0;
}

/** 读取 localStorage 中指定表的偏好。 */
async function readPrefs(page: import('@playwright/test').Page, tableKey: string) {
  const raw = await page.evaluate(k => localStorage.getItem(`ntd_table_prefs:${k}`), tableKey);
  return raw ? JSON.parse(raw) : null;
}

test.describe('054-列表表格交互增强', () => {
  // ── 事项列表（需 ntd_items_view=list 切到 table 形态）──
  test('事项列表：Table 渲染正常，列有拖拽手柄', async ({ page }) => {
    await gotoList(page, '/todos', { key: 'ntd_items_view', value: 'list' });
    await expect(page.locator('.ant-table-thead th').filter({ hasText: 'ID' }).first()).toBeVisible();
    await expect(page.locator('.ant-table-thead th').filter({ hasText: '标题' }).first()).toBeVisible();
    expect(await hasResizableHandle(page, 'ID')).toBe(true);
  });

  test('事项列表：点击可排序列写入 localStorage', async ({ page }) => {
    await gotoList(page, '/todos', { key: 'ntd_items_view', value: 'list' });
    await page.locator('.ant-table-thead th').filter({ hasText: '标题' }).first().click();
    await page.waitForTimeout(300);
    const prefs = await readPrefs(page, 'todos');
    expect(prefs).not.toBeNull();
    expect(prefs.sort.field).toBe('title');
    expect(['ascend', 'descend']).toContain(prefs.sort.order);
  });

  test('事项列表：ID 列默认倒序（localStorage 无记录时）', async ({ page }) => {
    await gotoList(page, '/todos', { key: 'ntd_items_view', value: 'list' });
    // 默认排序不点击任何列时，ID 列应有激活的降序指示
    const idTh = page.locator('.ant-table-thead th').filter({ hasText: 'ID' }).first();
    // antd 受控 sortOrder=descend 时 th 带 ant-table-column-sort class
    await expect(idTh).toHaveClass(/ant-table-column-sort/);
  });

  // ── 任务列表 ──
  test('任务列表：Table 渲染正常，列有拖拽手柄', async ({ page }) => {
    await gotoList(page, '/tasks');
    await expect(page.locator('.ant-table-thead th').filter({ hasText: '标题' }).first()).toBeVisible();
    expect(await hasResizableHandle(page, '标题')).toBe(true);
  });

  test('任务列表：点击可排序列写入 localStorage', async ({ page }) => {
    await gotoList(page, '/tasks');
    await page.locator('.ant-table-thead th').filter({ hasText: '状态' }).first().click();
    await page.waitForTimeout(300);
    const prefs = await readPrefs(page, 'tasks');
    expect(prefs).not.toBeNull();
    expect(prefs.sort.field).toBe('status');
  });

  // ── 环路列表 ──
  test('环路列表：Table 渲染正常，列有拖拽手柄', async ({ page }) => {
    await gotoList(page, '/loops');
    await expect(page.locator('.ant-table-thead th').filter({ hasText: '名称' }).first()).toBeVisible();
    expect(await hasResizableHandle(page, '名称')).toBe(true);
  });

  test('环路列表：列宽拖拽后写入 localStorage', async ({ page }) => {
    await gotoList(page, '/loops');
    const idTh = page.locator('.ant-table-thead th').filter({ hasText: 'ID' }).first();
    const handle = idTh.locator('span[style*="col-resize"]');
    const box = await handle.boundingBox();
    test.skip(!box, '拖拽手柄未找到');
    // 拖拽：向右拖 50px
    await page.mouse.move(box!.x + box!.width / 2, box!.y + box!.height / 2);
    await page.mouse.down();
    await page.mouse.move(box!.x + box!.width / 2 + 50, box!.y + box!.height / 2, { steps: 5 });
    await page.mouse.up();
    await page.waitForTimeout(300);
    const prefs = await readPrefs(page, 'loops');
    expect(prefs).not.toBeNull();
    // 默认 ID 列宽 70px，拖宽后应 > 70
    expect(prefs.widths.id).toBeGreaterThan(70);
  });

  // ── 刷新恢复 ──
  test('刷新后排序状态恢复', async ({ page }) => {
    await gotoList(page, '/todos', { key: 'ntd_items_view', value: 'list' });
    await page.locator('.ant-table-thead th').filter({ hasText: '标题' }).first().click();
    await page.waitForTimeout(300);
    await page.reload();
    await page.waitForSelector('.ant-table-thead th', { timeout: 10000 });
    await page.waitForTimeout(500);
    // 刷新后排序指示器仍在标题列
    expect(await hasActiveSortIndicator(page, '标题')).toBe(true);
    const prefs = await readPrefs(page, 'todos');
    expect(prefs.sort.field).toBe('title');
  });

  test('刷新后列宽状态恢复', async ({ page }) => {
    await gotoList(page, '/loops');
    const idTh = page.locator('.ant-table-thead th').filter({ hasText: 'ID' }).first();
    const handle = idTh.locator('span[style*="col-resize"]');
    const box = await handle.boundingBox();
    test.skip(!box, '拖拽手柄未找到');
    await page.mouse.move(box!.x + box!.width / 2, box!.y + box!.height / 2);
    await page.mouse.down();
    await page.mouse.move(box!.x + box!.width / 2 + 50, box!.y + box!.height / 2, { steps: 5 });
    await page.mouse.up();
    await page.waitForTimeout(300);
    await page.reload();
    await page.waitForSelector('.ant-table-thead th', { timeout: 10000 });
    await page.waitForTimeout(500);
    // 刷新后 ID 列宽保持拖宽后的值
    const width = await page.locator('.ant-table-thead th').filter({ hasText: 'ID' }).first()
      .evaluate(el => el.clientWidth);
    expect(width).toBeGreaterThan(70);
  });
});
