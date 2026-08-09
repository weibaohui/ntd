import { test, expect, type Page } from '@playwright/test';
import { execFileSync } from 'node:child_process';
import { homedir } from 'node:os';
import { join } from 'node:path';

const BASE = 'http://localhost:18088';
const DEV_DB = join(homedir(), '.ntd', 'data.dev.db');
const WORKSPACE_ID = 1;

// 测试直接向共享 dev 库插入数据（dev 库跨用例共享、无隔离），必须在批次结束时清理，
// 否则 guid-feat042-* 占位条目会永久残留：它们在「我的」工艺视图里 install/getProcess 返回 404，
// 排在列表首位时会打断任何取 .first() 工艺的链路（见 process_three_layer.spec.ts）。
// 按名称/guid 前缀 LIKE 清理（而非按本次 id），可顺带清掉历史运行残留的同类脏数据。
test.afterAll(() => {
  // 依赖方先删：loop_steps 引用 loops，故先于 loops；其余表互不外键依赖，顺序无关。
  const cleanup = `
DELETE FROM loop_steps WHERE loop_id IN (SELECT id FROM loops WHERE name LIKE 'FEAT042-loop-%');
DELETE FROM tasks WHERE title LIKE 'FEAT042-task-%';
DELETE FROM loops WHERE name LIKE 'FEAT042-loop-%';
DELETE FROM todos WHERE title LIKE 'FEAT042-todo-%';
DELETE FROM process_templates WHERE guid LIKE 'guid-feat042-%';
`;
  execFileSync('sqlite3', [DEV_DB, cleanup], { encoding: 'utf-8' });
});

// 自种子一条「todo → loop(带工艺) → task」链路，让事项/环路/任务三页都能命中同一条工艺列。
function seedProcessChain(tag: string): { expected: string; todoTitle: string; loopName: string; taskTitle: string } {
  const suffix = `${Date.now()}-${Math.floor(Math.random() * 1000)}`;
  const processName = `工艺统一-${tag}`;
  const todoTitle = `FEAT042-todo-${suffix}`;
  const loopName = `FEAT042-loop-${suffix}`;
  const taskTitle = `FEAT042-task-${suffix}`;
  const sql = `
INSERT INTO process_templates (guid, name, display_name, version, is_system, created_at, updated_at)
VALUES ('guid-feat042-${suffix}', 'feat042-${suffix}', '${processName}', '3.4.5', 0,
        strftime('%Y-%m-%dT%H:%M:%SZ','now','utc'), strftime('%Y-%m-%dT%H:%M:%SZ','now','utc'));
INSERT INTO todos (title, prompt, status, executor, workspace_id, workspace_path, created_at, updated_at)
VALUES ('${todoTitle}', '验证三列表工艺列统一', 'pending', 'claudecode', ${WORKSPACE_ID}, '/tmp',
        strftime('%Y-%m-%dT%H:%M:%SZ','now','utc'), strftime('%Y-%m-%dT%H:%M:%SZ','now','utc'));
INSERT INTO loops (name, description, workspace_id, workspace_path, status, process_template_id, process_template_version, created_at, updated_at)
VALUES ('${loopName}', '验证三列表工艺列统一', ${WORKSPACE_ID}, '/tmp', 'paused',
        (SELECT MAX(id) FROM process_templates), '3.4.5',
        strftime('%Y-%m-%dT%H:%M:%SZ','now','utc'), strftime('%Y-%m-%dT%H:%M:%SZ','now','utc'));
INSERT INTO loop_steps (loop_id, name, order_index, todo_id, enabled, gate_config, skill_names, created_at)
VALUES ((SELECT MAX(id) FROM loops), '生成 PRD', 0, (SELECT MAX(id) FROM todos), 1, '[]', '[]',
        strftime('%Y-%m-%dT%H:%M:%SZ','now','utc'));
INSERT INTO tasks (title, description, status, workspace_id, template_id, loop_id, created_by, created_at, updated_at)
VALUES ('${taskTitle}', '验证三列表工艺列统一', 'pending', ${WORKSPACE_ID},
        (SELECT MAX(id) FROM process_templates), (SELECT MAX(id) FROM loops), 'playwright',
        strftime('%Y-%m-%dT%H:%M:%SZ','now','utc'), strftime('%Y-%m-%dT%H:%M:%SZ','now','utc'));
SELECT (SELECT MAX(id) FROM process_templates) || ',' || '${processName}';
`;
  const out = execFileSync('sqlite3', [DEV_DB, sql], { encoding: 'utf-8' }).trim();
  const [id, name] = out.split(',');
  return { expected: `#${id}-${name}-3.4.5`, todoTitle, loopName, taskTitle };
}

async function openPage(page: Page, hash: string) {
  await page.addInitScript(() => {
    localStorage.setItem('app_theme', 'light');
    // 事项/环路列表都直接读 selectedWorkspace；固定为种子数据所在工作空间，避免空选 workspace 导致空表。
    localStorage.setItem('selected_workspace', String(1));
    localStorage.setItem('ntd_items_view', 'list');
    localStorage.setItem('ntd_tasks_view', 'list');
  });
  await page.goto(`${BASE}/#${hash}`, { waitUntil: 'domcontentloaded' });
}

test('FEAT-042：事项/任务/环路列表工艺列统一为 #id-名称-版本', async ({ page }) => {
  const { expected, todoTitle, loopName, taskTitle } = seedProcessChain('三列表');

  await openPage(page, '/todos');
  await expect(page.getByText(todoTitle)).toBeVisible();
  await expect(page.getByText(expected)).toBeVisible();

  await openPage(page, '/tasks');
  await expect(page.getByText(taskTitle)).toBeVisible();
  await expect(page.getByText(expected)).toBeVisible();

  await openPage(page, '/loops');
  await expect(page.getByText(loopName)).toBeVisible();
  await expect(page.getByText(expected)).toBeVisible();
});
