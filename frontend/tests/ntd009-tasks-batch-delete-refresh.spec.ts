import { test, expect, type Page } from '@playwright/test';
import { execFileSync } from 'node:child_process';
import { homedir } from 'node:os';
import { join } from 'node:path';

const BASE = 'http://localhost:18088';
const DEV_DB = join(homedir(), '.ntd', 'data.dev.db');
const WORKSPACE_ID = 1;

// 自种子任务数据：批量删除会真实删行，独立数据可避免影响其他用例。
function seedBatchTask(tag: string): { taskId: number; title: string } {
  const title = `NTD009-${tag}-${Date.now()}-${Math.floor(Math.random() * 1000)}`;
  const sql = `
INSERT INTO tasks (title, description, status, workspace_id, created_by, created_at, updated_at)
VALUES ('${title}', '验证任务批量删除后列表自动刷新', 'pending', ${WORKSPACE_ID}, 'playwright',
        strftime('%Y-%m-%dT%H:%M:%SZ','now','utc'), strftime('%Y-%m-%dT%H:%M:%SZ','now','utc'));
SELECT MAX(id) FROM tasks;
`;
  const out = execFileSync('sqlite3', [DEV_DB, sql], { encoding: 'utf-8' }).trim();
  return { taskId: Number(out), title };
}

async function openTasksPage(page: Page) {
  // workspace 必须显式锁定到 WORKSPACE_ID：DataLoader 启动时按 path 升序取 dirs[0]
  // （dev 库里 /Users/mac/sticky-notes 排最前 → ws 3），而种子任务在 ws 1；
  // 不锁定会让 TasksPage 加载 ws 3 列表，种子行永远不出现，删除前 toHaveCount(1) 即失败。
  await page.addInitScript((wsId) => {
    localStorage.setItem('app_theme', 'light');
    localStorage.setItem('ntd_tasks_view', 'list');
    localStorage.setItem('selected_workspace', String(wsId));
  }, WORKSPACE_ID);
  await page.goto(`${BASE}/#/tasks`, { waitUntil: 'domcontentloaded' });
  await expect(page.getByTestId('tasks-table-view')).toBeVisible();
}

async function selectTaskRow(page: Page, title: string) {
  await page.getByRole('row', { name: new RegExp(title) }).getByRole('checkbox').check();
}

async function confirmBatchDelete(page: Page) {
  await page.getByTestId('tasks-table-batch-trigger').click();
  await page.getByRole('menuitem', { name: '删除' }).click();
  await expect(page.locator('.ant-modal-confirm-title')).toHaveText('确认删除 2 个任务？');
  await page.getByRole('button', { name: /删\s*除/ }).click();
}

test('NTD-009：任务批量删除成功后列表自动刷新', async ({ page }) => {
  const first = seedBatchTask('刷新A');
  const second = seedBatchTask('刷新B');
  await openTasksPage(page);

  // 删除前先确认两行都在表格中，避免把「本来就不存在」误判成刷新成功。
  await expect(page.getByRole('row', { name: new RegExp(first.title) })).toHaveCount(1);
  await expect(page.getByRole('row', { name: new RegExp(second.title) })).toHaveCount(1);

  await selectTaskRow(page, first.title);
  await selectTaskRow(page, second.title);
  await confirmBatchDelete(page);

  // 核心回归断言：删除成功后父组件应重拉列表，两条已删任务行应从表格消失。
  await expect(page.getByText('已删除 2 个任务')).toBeVisible({ timeout: 5000 });
  await expect(page.getByRole('row', { name: new RegExp(first.title) })).toHaveCount(0, { timeout: 10000 });
  await expect(page.getByRole('row', { name: new RegExp(second.title) })).toHaveCount(0, { timeout: 10000 });
});
