// 114 功能验证：事项表全列（除操作列）表头服务端排序
import { test, expect } from '@playwright/test';

const SORT_COLUMNS: [string, string][] = [
  ['ID', 'id'],
  ['类型', 'type'],
  ['标题', 'title'],
  ['状态', 'status'],
  ['执行器', 'executor'],
  ['专家', 'expert_name'],
  ['调度', 'scheduler'],
  ['环路', 'loop'],
  ['工艺', 'process'],
  ['最近执行', 'last_execution_status'],
  ['执行时间', 'last_execution_at'],
  ['更新时间', 'updated_at'],
];

test('114 事项表：12 列全部可排 + 请求参数正确', async ({ page }) => {
  const reqs: string[] = [];
  page.on('request', (r) => { if (r.url().includes('/todos/center')) reqs.push(r.url()); });
  await page.goto('http://localhost:18088/#/todos?view=list', { waitUntil: 'networkidle' });
  await page.waitForTimeout(1500);

  const sortable = await page.locator('.ant-table-thead th .ant-table-column-sorters').count();
  expect(sortable).toBe(12);

  for (const [label, sortBy] of SORT_COLUMNS) {
    const h = page.locator('.ant-table-thead th', { hasText: label }).first();
    // 起点状态未知（localStorage 持久化），循环点击最多 3 次覆盖「取消→升→降」三态
    let ascSeen = false;
    let descSeen = false;
    for (let i = 0; i < 3; i++) {
      await h.click();
      await page.waitForTimeout(700);
      const url = reqs[reqs.length - 1] ?? '';
      if (url.includes(`sort_by=${sortBy}`)) {
        if (url.includes('sort_order=asc')) ascSeen = true;
        if (url.includes('sort_order=desc')) descSeen = true;
        if (ascSeen && descSeen) break;
      }
    }
    expect(ascSeen, `${label} 应出现 asc 请求`).toBe(true);
    expect(descSeen, `${label} 应出现 desc 请求`).toBe(true);
  }
  expect(await page.locator('.ant-table-thead th.ant-table-column-sort').count()).toBe(1);
});

test('114 事项表：更新时间/执行时间倒序首行与 API 一致', async ({ page }) => {
  const reqs: string[] = [];
  page.on('request', (r) => { if (r.url().includes('/todos/center')) reqs.push(r.url()); });
  await page.goto('http://localhost:18088/#/todos?view=list', { waitUntil: 'networkidle' });
  await page.waitForTimeout(1200);
  for (const [label, sortBy] of [['更新时间', 'updated_at'], ['执行时间', 'last_execution_at']] as const) {
    const h = page.locator('.ant-table-thead th', { hasText: label }).first();
    let descSeen = false;
    for (let i = 0; i < 3; i++) {
      await h.click();
      await page.waitForTimeout(800);
      const url = reqs[reqs.length - 1] ?? '';
      if (url.includes(`sort_by=${sortBy}`) && url.includes('sort_order=desc')) { descSeen = true; break; }
    }
    expect(descSeen, `${label} 应到达 desc`).toBe(true);
    const firstRowId = (await page.locator('.ant-table-row').first().locator('td').nth(1).innerText()).replace('#', '');
    const resp = await page.request.get(
      `http://localhost:18088/api/v1/workspaces/1/todos/center?page=1&page_size=20&sort_by=${sortBy}&sort_order=desc`
    );
    const json = await resp.json();
    expect(Number(firstRowId), `${label} 首行应匹配 API`).toBe(json.data.items[0].id);
    console.log(`${label} desc 首行 id: ${firstRowId} | API: ${json.data.items[0].id}`);
  }
});
