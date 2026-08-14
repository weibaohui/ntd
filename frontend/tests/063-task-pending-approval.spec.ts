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
 * - 种子前缀每次运行唯一（见 beforeAll）：dev 库被全量套件共享，固定前缀 +
 *   beforeAll/afterAll 清理会在并行 worker 同跑本文件时互删对方种子（自竞态）。
 */

import { test, expect } from '@playwright/test';
import { execFileSync } from 'node:child_process';
import { homedir } from 'node:os';
import { join } from 'node:path';

// ── 常量 ──
const BASE = 'http://localhost:18088';
const DEV_DB = join(homedir(), '.ntd', 'data.dev.db');
const WS = 1;
// 本次运行的种子前缀，beforeAll 里生成（见下）：固定值会让并行 worker 的 cleanup()
// 按 LIKE 误删对方刚种的行；run 唯一前缀保证清理只碰自己的数据，互不干扰。
let PREFIX = 'e2e-063-';

/**
 * 带 busy_timeout 的 sqlite3 执行：dev 库同时被后端服务占用，写锁冲突时
 * sqlite3 CLI 默认立即报 database is locked(5)；busy_timeout 让本次连接
 * 挂起等锁（最多 5s）而非直接失败，覆盖测试自身并行写入与后端写事务的竞争窗口。
 * 实现细节两个坑：
 * - 不能用 PRAGMA busy_timeout：它会向 stdout 输出一行 "5000"，污染 SELECT
 *   回读（Number("5000\n320") → NaN，063-3/4/5 曾因此全挂）。
 * - dot-command 必须走 -cmd 选项：混进位置参数会被当 SQL 静默吞掉整段脚本
 *   （退出码仍为 0，种子不落库且难排查）。
 */
function runSql(sql: string): string {
  return execFileSync('sqlite3', ['-cmd', '.timeout 5000', DEV_DB, sql], {
    encoding: 'utf-8',
  });
}

/**
 * 种子数据：一个 task + 其 loop 的一次执行，环节停在 pending_approval。
 * approval_status 同时写 'pending'（旧评分路径），验证 OR 口径不重复计数 → 计数应为 1。
 */
function seedPendingApprovalTask(): number {
  const ts = Date.now();
  // 关联 id 一律用 last_insert_rowid() 链式回填，不用 (SELECT MAX(id) ...)：
  // dev 库被多个并行 worker 共享，MAX(id) 在「本 worker INSERT 与取 MAX 之间」
  // 可能被对方 worker 插入的行反超（A 的 task 挂到 B 的 loop 上，清理时被连带删除）。
  // last_insert_rowid() 只看本连接会话的最后插入，天然 worker 隔离。
  runSql(`
    INSERT INTO todos (title, prompt, status, executor, workspace_id, workspace_path, created_at, updated_at)
    VALUES ('${PREFIX}todo-${ts}','待审批环节载体','success','claudecode',${WS},'/tmp',
            datetime('now'), datetime('now'));
    INSERT INTO loops (name,description,workspace_id,workspace_path,status,created_at,updated_at)
    VALUES ('${PREFIX}loop-${ts}','063 待审批透出验证',${WS},'/tmp','enabled',
            datetime('now'), datetime('now'));
    INSERT INTO loop_steps (loop_id,name,description,order_index,todo_id,gate_config,skill_names,created_at)
    VALUES (last_insert_rowid(),'人工审批环节','需人工审批',0,
            (SELECT id FROM todos WHERE title='${PREFIX}todo-${ts}'),
            '[{"name":"人工审批","type":"human_approval"}]','[]', datetime('now'));
    INSERT INTO tasks (title,description,status,workspace_id,loop_id,created_by,created_at,updated_at)
    VALUES ('${PREFIX}task-${ts}','063 待审批透出验证任务','running',${WS},
            (SELECT id FROM loops WHERE name='${PREFIX}loop-${ts}'),'e2e',datetime('now'),datetime('now'));
    INSERT INTO loop_executions (loop_id,trigger_type,trigger_meta,started_at,status,total_steps,task_id)
    VALUES ((SELECT id FROM loops WHERE name='${PREFIX}loop-${ts}'),'manual','{}',datetime('now'),
            'running',1,(SELECT id FROM tasks WHERE title='${PREFIX}task-${ts}'));
    INSERT INTO loop_step_executions (loop_execution_id,step_id,todo_id,status,started_at,
                                      sequence_index,approval_status)
    VALUES ((SELECT id FROM loop_executions WHERE loop_id=(SELECT id FROM loops WHERE name='${PREFIX}loop-${ts}')),
            (SELECT id FROM loop_steps WHERE loop_id=(SELECT id FROM loops WHERE name='${PREFIX}loop-${ts}')),
            (SELECT id FROM todos WHERE title='${PREFIX}todo-${ts}'),
            'pending_approval',datetime('now'),1,'pending');
    INSERT INTO loop_step_execution_gates (loop_step_execution_id,gate_type,gate_name,config,status)
    VALUES ((SELECT id FROM loop_step_executions WHERE loop_execution_id=
              (SELECT id FROM loop_executions WHERE loop_id=(SELECT id FROM loops WHERE name='${PREFIX}loop-${ts}'))),
            'human_approval','人工审批','{"name":"人工审批","type":"human_approval"}','pending');
  `);
  // 回读 task id 也按 run 唯一标题精确匹配，避免 LIKE 前缀误中并行 worker 的行。
  return Number(runSql(
    `SELECT id FROM tasks WHERE title='${PREFIX}task-${ts}'`
  ).trim());
}

/** 清理本次运行的种子数据（自外而内按 FK 依赖顺序删除，仅命中自己的 run 前缀）。 */
function cleanup(): void {
  runSql(`
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
  `);
}

/**
 * 固定任务页视图模式后打开任务页。
 *
 * 关键实现：用 addInitScript 在任何应用代码执行前写入 localStorage，
 * 不能用「goto → evaluate → reload」三段式：应用启动后 useApp/App.tsx 的异步
 * SELECT_WORKSPACE dispatch 会把 evaluate 刚写入的值覆盖回 dirs[0]（dev 库按
 * path 排序是 ws3），reload 读到 3 而非种子所在的 WS=1，任务行/卡片随机消失
 * （并发下 dispatch 与 evaluate 的先后顺序不确定，表现为偶发失败）。
 * addInitScript 先于应用 boot 执行，getInitialWorkspace 直接读到 WS。
 */
async function gotoTasks(page: import('@playwright/test').Page, mode: 'list' | 'kanban' | 'card') {
  // addInitScript 回调会被序列化到浏览器执行，无法闭包捕获外层 WS，必须随 arg 显式传入。
  await page.addInitScript(
    ({ m, ws }) => {
      localStorage.setItem('ntd_tasks_view', m);
      // 钉住工作空间：种子任务在 WS，浏览器请求必须命中同一 workspace 才能看到该任务。
      localStorage.setItem('selected_workspace', String(ws));
    },
    { m: mode, ws: WS },
  );
  await page.goto(`${BASE}/#/tasks`, { waitUntil: 'domcontentloaded' });
  await page.waitForTimeout(2000);
}

let taskId = 0;

test.beforeAll(() => {
  // run 唯一前缀：TEST_PARALLEL_INDEX 区分同文件并行 worker，时间戳+随机数
  // 兜底同 worker 内 repeat-each 重复运行（beforeAll 每轮都会重新触发清理+种入）。
  PREFIX = `e2e-063-w${process.env.TEST_PARALLEL_INDEX ?? 'x'}-${Date.now()}-${Math.random().toString(36).slice(2, 6)}-`;
  cleanup();
  taskId = seedPendingApprovalTask();
});
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
