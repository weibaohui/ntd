import { test, expect, type Page } from '@playwright/test';
import { execFileSync } from 'node:child_process';
import { homedir } from 'node:os';
import { join } from 'node:path';

const BASE = 'http://localhost:18088';
const DEV_DB = join(homedir(), '.ntd', 'data.dev.db');
const WORKSPACE_ID = 1;

type ThemeName = 'light' | 'dark';

// 自种子一条任务列表数据：用例不依赖手工 dev 库状态，重复执行也能稳定找到目标行。
function seedBatchTask(tag: string): { taskId: number; title: string } {
  const title = `NTD006-${tag}-${Date.now()}`;
  const sql = `
INSERT INTO tasks (title, description, status, workspace_id, created_by, created_at, updated_at)
VALUES ('${title}', '验证任务页批量按钮顺序与删除确认框主题', 'pending', ${WORKSPACE_ID}, 'playwright',
        strftime('%Y-%m-%dT%H:%M:%SZ','now','utc'), strftime('%Y-%m-%dT%H:%M:%SZ','now','utc'));
SELECT MAX(id) FROM tasks;
`;
  const out = execFileSync('sqlite3', [DEV_DB, sql], { encoding: 'utf-8' }).trim();
  return { taskId: Number(out), title };
}

// 打开任务页前写入主题与视图偏好：ThemeProvider 初始化时读取 localStorage，
// 这样能稳定覆盖亮/暗两个分支，而不依赖操作系统主题。
async function openTasksPage(page: Page, theme: ThemeName, title: string) {
  await page.addInitScript((nextTheme) => {
    localStorage.setItem('app_theme', nextTheme);
    localStorage.setItem('ntd_tasks_view', 'list');
  }, theme);
  await page.goto(`${BASE}/#/tasks`, { waitUntil: 'domcontentloaded' });
  await page.waitForFunction((nextTheme) => document.documentElement.getAttribute('data-theme') === nextTheme, theme);
  await expect(page.getByTestId('tasks-table-view')).toBeVisible();
  await expect(page.getByText(title)).toBeVisible();
}

// 勾选目标任务行：批量按钮只在有选中行时渲染，因此每个用例都先走同一前置动作。
async function selectTaskRow(page: Page, title: string) {
  // 用行文本先锁定数据行，再在行内找 checkbox；表格里还存在表头全选/测量用 checkbox，
  // 直接用 tbody 全局第一个 checkbox 会误点到「Select all」。
  await page.getByRole('row', { name: new RegExp(title) }).getByRole('checkbox').check();
  await expect(page.getByTestId('tasks-table-batch-trigger')).toBeVisible();
}

// 打开批量删除确认窗，但不断言删除结果；本用例只验证确认窗本身是否进入当前主题上下文。
async function openBatchDeleteConfirm(page: Page) {
  await page.getByTestId('tasks-table-batch-trigger').click();
  await page.getByRole('menuitem', { name: '删除' }).click();
  await expect(page.locator('.ant-modal-confirm')).toBeVisible();
}

async function readModalTheme(page: Page) {
  // AntD v6 的确认窗容器类名是 .ant-modal-container（普通 Modal 仍是 .ant-modal-content）。
  const content = page.locator('.ant-modal-confirm .ant-modal-container');
  const title = page.locator('.ant-modal-confirm-title');
  return {
    background: await content.evaluate((el) => getComputedStyle(el).backgroundColor),
    titleColor: await title.evaluate((el) => getComputedStyle(el).color),
  };
}

test('NTD-006：勾选任务后批量按钮位于工具栏第一位', async ({ page }) => {
  const { title } = seedBatchTask('顺序');
  await openTasksPage(page, 'light', title);
  await selectTaskRow(page, title);

  const toolbarTestIds = await page.locator('[data-testid="tasks-table-view"] > div').first().evaluate((toolbar) => {
    return Array.from(toolbar.children).map((child) => child.getAttribute('data-testid'));
  });
  expect(toolbarTestIds[0]).toBe('tasks-table-batch-trigger');
});

test('NTD-006：批量删除确认窗适配亮色主题', async ({ page }) => {
  const { title } = seedBatchTask('亮色');
  await openTasksPage(page, 'light', title);
  await selectTaskRow(page, title);
  await openBatchDeleteConfirm(page);

  await expect(page.locator('.ant-modal-confirm-title')).toHaveText('确认删除 1 个任务？');
  expect(await readModalTheme(page)).toEqual({
    background: 'rgb(255, 255, 255)',
    titleColor: 'rgb(15, 23, 42)',
  });
  await page.getByRole('button', { name: /取\s*消/ }).click();
});

test('NTD-006：批量删除确认窗适配暗色主题', async ({ page }) => {
  const { title } = seedBatchTask('暗色');
  await openTasksPage(page, 'dark', title);
  await selectTaskRow(page, title);
  await openBatchDeleteConfirm(page);

  await expect(page.locator('.ant-modal-confirm-title')).toHaveText('确认删除 1 个任务？');
  expect(await readModalTheme(page)).toEqual({
    background: 'rgb(30, 30, 46)',
    titleColor: 'rgb(205, 214, 244)',
  });
  await page.getByRole('button', { name: /取\s*消/ }).click();
});
