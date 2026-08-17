// 114 验证：任务/环路表格补充列可排
import { test, expect } from '@playwright/test';

test('114 任务表：工艺/最近执行列可排', async ({ page }) => {
  await page.goto('http://localhost:18088/#/tasks?view=list', { waitUntil: 'networkidle' });
  await page.waitForTimeout(1500);
  const headers = await page.locator('.ant-table-thead th').allTextContents();
  const sortable = await page.locator('.ant-table-thead th .ant-table-column-sorters').count();
  console.log('任务表头:', JSON.stringify(headers.map(h => h.trim())));
  console.log('任务可排序列数:', sortable);
  // 工艺与最近执行应可排
  for (const label of ['工艺', '最近执行']) {
    const th = page.locator('.ant-table-thead th', { hasText: label }).first();
    expect(await th.locator('.ant-table-column-sorters').count()).toBe(1);
  }
  // 点击工艺表头两次（asc→desc），行序应变化（客户端排序）
  const firstBefore = await page.locator('.ant-table-row').first().locator('td').nth(1).innerText();
  const th = page.locator('.ant-table-thead th', { hasText: '工艺' }).first();
  await th.click(); await page.waitForTimeout(600);
  await th.click(); await page.waitForTimeout(600);
  const firstAfter = await page.locator('.ant-table-row').first().locator('td').nth(1).innerText();
  console.log(`工艺排序前后首行ID: ${firstBefore} -> ${firstAfter}`);
});

test('114 环路表：工艺/最近执行列可排', async ({ page }) => {
  await page.goto('http://localhost:18088/#/loops?view=list', { waitUntil: 'networkidle' });
  await page.waitForTimeout(1500);
  const sortable = await page.locator('.ant-table-thead th .ant-table-column-sorters').count();
  console.log('环路可排序列数:', sortable);
  for (const label of ['工艺', '最近执行']) {
    const th = page.locator('.ant-table-thead th', { hasText: label }).first();
    expect(await th.locator('.ant-table-column-sorters').count()).toBe(1);
  }
});
