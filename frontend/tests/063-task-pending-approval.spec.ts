/**
 * 063 任务待审批状态透出 — UI 端到端验证。
 *
 * 测试范围：
 * 1. 任务列表接口下发 pending_approval_count
 * 2. 列表视图：「待审批」列显示红色「N 待审批」标记
 * 3. 看板视图：待审批任务进入首列「待审批」泳道
 * 4. 卡片视图：卡片头部显示「N 待审批」标记
 * 5. 点击标记 → 详情执行历史 Tab 自动展开待审批执行，「通过/拒绝」按钮可见
 *
 * 数据准备：sqlite3 直落 pending_approval 环节执行（含 approval_status='pending'
 * 与 human_approval gate pending，与 044 spec 同模式，不依赖真实执行器）。
 *
 * 关键约定：
 * - workspace_id=1 匹配默认工作空间；
 * - 任务页视图模式经 localStorage ntd_tasks_view 固定，避免依赖上次浏览残留。
 */

import { test, expect } from '@playwright/test';
import { execFileSync } from 'node:child_process';
import { homedir } from 'node:os';
import { join } from 'node:path';

// ── 常量 ──
const BASE = 'http://localhost:18088';
const DEV_DB = join(homedir(), '.ntd', 'data.dev.db');
const WS = 1;
const PREFIX = 'e2e-063-';

/**
 * 种子数据：一个 task + 其 loop 的一次执行，环节停在 pending_approval。
 * approval_status 同时写 'pending'（旧评分路径），验证 OR 口径不重复计数 → 计数应为 1。
 */
function seedPendingApprovalTask(): number {
  const ts = Date.now();
  execFileSync('sqlite3', [DEV_DB, `
    INSERT INTO todos (title, prompt, status, executor, workspace_id, workspace_path, created_at, updated_at)
    VALUES ('${PREFIX}todo-${ts}','待审批环节载体','success','claudecode',${WS},'/tmp',
            datetime('now'), datetime('now'));
    INSERT INTO loops (name,description,workspace_id,workspace_path,status,created_at,updated_at)
    VALUES ('${PREFIX}loop-${ts}','063 待审批透出验证',${WS},'/tmp','enabled',
            datetime('now'), datetime('now'));
    INSERT INTO loop_steps (loop_id,name,description,order_index,todo_id,gate_config,skill_names,created_at)
    VALUES ((SELECT MAX(id) FROM loops),'人工审批环节','需人工审批',0,
            (SELECT MAX(id) FROM todos),
            '[{"name":"人工审批","type":"human_approval"}]','[]', datetime('now'));
    INSERT INTO tasks (title,description,status,workspace_id,loop_id,created_by,created_at,updated_at)
    VALUES ('${PREFIX}task-${ts}','063 待审批透出验证任务','running',${WS},
            (SELECT MAX(id) FROM loops),'e2e',datetime('now'),datetime('now'));
    INSERT INTO loop_executions (loop_id,trigger_type,trigger_meta,started_at,status,total_steps,task_id)
    VALUES ((SELECT MAX(id) FROM loops),'manual','{}',datetime('now'),'running',1,
            (SELECT MAX(id) FROM tasks));
    INSERT INTO loop_step_executions (loop_execution_id,step_id,todo_id,status,started_at,
                                      sequence_index,approval_status)
    VALUES ((SELECT MAX(id) FROM loop_executions),(SELECT MAX(id) FROM loop_steps),
            (SELECT MAX(id) FROM todos),'pending_approval',datetime('now'),1,'pending');
    INSERT INTO loop_step_execution_gates (loop_step_execution_id,gate_type,gate_name,config,status)
    VALUES ((SELECT MAX(id) FROM loop_step_executions),
            'human_approval','人工审批','{"name":"人工审批","type":"human_approval"}','pending');
  `], { encoding: 'utf-8' });
  return Number(execFileSync('sqlite3', [DEV_DB,
    `SELECT id FROM tasks WHERE title LIKE '${PREFIX}task-%' ORDER BY id DESC LIMIT 1`
  ], { encoding: 'utf-8' }).trim());
}

/** 清理 e2e-063- 种子数据（自外而内按 FK 依赖顺序删除）。 */
function cleanup(): void {
  execFileSync('sqlite3', [DEV_DB, `
    DELETE FROM loop_step_execution_gates WHERE loop_step_execution_id IN
      (SELECT id FROM loop_step_executions WHERE loop_execution_id IN
        (SELECT id FROM loop_executions WHERE loop_id IN
          (SELECT id FROM loops WHERE name LIKE '${PREFIX}%')));
    DELETE FROM loop_step_executions WHERE loop_execution_id IN
      (SELECT id FROM loop_executions WHERE loop_id IN
        (SELECT id FROM loops WHERE name LIKE '${PREFIX}%'));
    DELETE FROM loop_executions WHERE loop_id IN
      (SELECT id FROM loops WHERE name LIKE '${PREFIX}%');
    DELETE FROM tasks WHERE title LIKE '${PREFIX}%';
    DELETE FROM loop_steps WHERE loop_id IN
      (SELECT id FROM loops WHERE name LIKE '${PREFIX}%');
    DELETE FROM loops WHERE name LIKE '${PREFIX}%';
    DELETE FROM todos WHERE title LIKE '${PREFIX}%';
  `], { encoding: 'utf-8' });
}

/**
 * 固定任务页视图模式后打开任务页。
 *
 * 关键修复：同时把 selected_workspace 钉到 WS(=1)。
 * 应用启动时 useTodoContext 的 getInitialWorkspace 会读 localStorage 的 selected_workspace
 * 决定请求哪个 workspace 的任务列表；该值会被其他 e2e 用例（或上一次手动操作）残留成非 1 的 id，
 * 导致本用例种子落到 ws=1、浏览器却请求 ws=N，列表/看板/卡片三态都看不到任务、待审批标记自然不渲染。
 * 种子与接口断言（063-1）都按 WS=1 走，这里必须把浏览器也钉回 WS 保持一致。
 */
async function gotoTasks(page: import('@playwright/test').Page, mode: 'list' | 'kanban' | 'card') {
  await page.goto(`${BASE}/#/tasks`, { waitUntil: 'domcontentloaded' });
  // page.evaluate 的回调会被序列化到浏览器执行，无法闭包捕获外层 WS，必须随 arg 显式传入。
  await page.evaluate(
    ({ m, ws }) => {
      localStorage.setItem('ntd_tasks_view', m);
      // 钉住工作空间：种子任务在 WS，浏览器请求必须命中同一 workspace 才能看到该任务。
      localStorage.setItem('selected_workspace', String(ws));
    },
    { m: mode, ws: WS },
  );
  await page.reload({ waitUntil: 'domcontentloaded' });
  await page.waitForTimeout(2000);
}

let taskId = 0;

test.beforeAll(() => { cleanup(); taskId = seedPendingApprovalTask(); });
test.afterAll(() => cleanup());

// ==========================================================
// 1. 接口：pending_approval_count 下发且按行计一次（OR 不重复计数）
// ==========================================================
test('063-1 列表接口下发待审批计数', async ({ request }) => {
  const res = await request.get(`${BASE}/api/v1/workspaces/${WS}/tasks`);
  expect(res.ok()).toBeTruthy();
  const body = await res.json();
  const items = body.data ?? body;
  const mine = items.find((t: { id: number }) => t.id === taskId);
  expect(mine, '种子任务应在列表中').toBeTruthy();
  // status 与 approval_status 同时命中同一行，OR 口径应计 1 而非 2。
  expect(mine.pending_approval_count).toBe(1);
});

// ==========================================================
// 2. 列表视图：「待审批」列红色标记
// ==========================================================
test('063-2 列表视图待审批列可见', async ({ page }) => {
  await gotoTasks(page, 'list');
  // 行内待审批标记（Tag 自带 data-testid）。
  const tag = page.locator(`tr:has-text("${PREFIX}task-")`).getByTestId('pending-approval-tag');
  await expect(tag).toBeVisible({ timeout: 8000 });
  await expect(tag).toContainText('1 待审批');
});

// ==========================================================
// 3. 看板视图：任务进入「待审批」泳道
// ==========================================================
test('063-3 看板视图待审批泳道承载任务卡片', async ({ page }) => {
  await gotoTasks(page, 'kanban');
  await expect(page.locator(`[data-testid="tasks-kanban-card-${taskId}"]`)).toBeVisible({ timeout: 8000 });
  // 卡片在「待审批」泳道列内（列头文字 + 卡片同列容器）。
  const lane = page.locator('div').filter({ has: page.locator(`[data-testid="tasks-kanban-card-${taskId}"]`) })
    .filter({ hasText: '待审批' }).last();
  await expect(lane).toBeVisible();
  await expect(page.locator(`[data-testid="tasks-kanban-card-${taskId}"]`).getByTestId('pending-approval-tag'))
    .toContainText('1 待审批');
});

// ==========================================================
// 4. 卡片视图：卡片头部红色标记
// ==========================================================
test('063-4 卡片视图待审批标记可见', async ({ page }) => {
  await gotoTasks(page, 'card');
  const card = page.locator(`[data-testid="tasks-card-${taskId}"]`);
  await expect(card).toBeVisible({ timeout: 8000 });
  await expect(card.getByTestId('pending-approval-tag')).toContainText('1 待审批');
});

// ==========================================================
// 5. 点击标记 → 详情执行历史 Tab 自动展开待审批执行
// ==========================================================
test('063-5 点击标记直达执行历史并自动展开审批区', async ({ page }) => {
  await gotoTasks(page, 'card');
  const tag = page.locator(`[data-testid="tasks-card-${taskId}"]`).getByTestId('pending-approval-tag');
  await expect(tag).toBeVisible({ timeout: 8000 });
  await tag.click();
  // URL 应落到任务详情执行历史 Tab。
  await page.waitForTimeout(2500);
  expect(page.url()).toContain(`tab=exec`);
  // 待审批执行行自动展开后，人工审批操作区（通过/拒绝）应直接可见。
  // 注意：antd 对两汉字按钮自动插入空格（「通 过」），用宽松正则匹配。
  await expect(page.locator('button').filter({ hasText: /通\s*过/ }).first()).toBeVisible({ timeout: 10000 });
  await expect(page.locator('button').filter({ hasText: /拒\s*绝/ }).first()).toBeVisible();
});
