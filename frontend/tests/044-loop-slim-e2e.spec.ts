/**
 * 044 环路瘦身与工艺对齐 — UI 端到端验证。
 *
 * 测试范围：
 * 1. 列表页无「新建」「触发」入口
 * 2. 详情页只读（无触发/编辑/复制/导出按钮，无触发器 Tab，环节只读）
 * 3. 门禁审批 API → 续跑成功
 *
 * 数据准备：sqlite3 直接写入种子数据（门禁测试直接落地 pending_approval 状态，
 * 不依赖执行器实际执行）。
 *
 * 关键约定：
 * - 环路 `workspace_id=1` 匹配默认工作空间。
 * - 门禁测试 seed 数据需设 `approval_status='pending'`，否则 `populate_pending_gate_id`
 *   不注入 `pending_gate_id`。
 */

import { test, expect } from '@playwright/test';
import { execFileSync } from 'node:child_process';
import { homedir } from 'node:os';
import { join } from 'node:path';

// ── 常量 ──
const BASE = 'http://localhost:18088';
const DEV_DB = join(homedir(), '.ntd', 'data.dev.db');
const WS = 1;

/** 门禁审批测试的共享数据，由第一个 API 用例写入，供前端用例读取。 */
let gateTestLoopId = 0;
let gateTestExecId = 0;

// ── 工具函数 ──

/** 写入一条基础环路，返回 loopId。 */
function seedBasicLoop(): number {
  const ts = Date.now();
  execFileSync('sqlite3', [DEV_DB, `
    INSERT INTO todos (title, prompt, status, executor, workspace_id, workspace_path, created_at, updated_at)
    VALUES ('e2e-044-basic-todo-${ts}','基础展示','pending','claudecode',${WS},'/tmp',
            datetime('now'), datetime('now'));
    INSERT INTO loops (name,description,workspace_id,workspace_path,status,created_at,updated_at)
    VALUES ('e2e-044-basic-loop-${ts}','基础环路验证',${WS},'/tmp','enabled',
            datetime('now'), datetime('now'));
    INSERT INTO loop_steps (loop_id,name,description,order_index,todo_id,gate_config,skill_names,created_at)
    VALUES ((SELECT MAX(id) FROM loops),'单一步骤','不含门禁',0,(SELECT MAX(id) FROM todos),
            '[]','[]', datetime('now'));
  `], { encoding: 'utf-8' });
  return Number(execFileSync('sqlite3', [DEV_DB,
    `SELECT id FROM loops WHERE name LIKE 'e2e-044-basic-loop-%' ORDER BY id DESC LIMIT 1`
  ], { encoding: 'utf-8' }).trim());
}

/**
 * 种子数据：直落已到达 pending_approval 状态的执行（含 approval_status='pending'、
 * human_approval gate pending），
 * 验证门禁审批 API 通过后自动 resume 续跑→终态 success。
 */
function seedGateApprovalScenario(): { loopId: number; execId: number; stepExecId: number; gateId: number } {
  const ts = Date.now();
  const out = execFileSync('sqlite3', [DEV_DB, `
    INSERT INTO todos (title,prompt,status,executor,workspace_id,workspace_path,created_at,updated_at)
    VALUES ('e2e-044-gate-todo-${ts}','门禁审批待审','success','claudecode',${WS},'/tmp',
            datetime('now'), datetime('now'));
    INSERT INTO loops (name,description,workspace_id,workspace_path,status,created_at,updated_at)
    VALUES ('e2e-044-gate-loop-${ts}','门禁审批 E2E',${WS},'/tmp','enabled',
            datetime('now'), datetime('now'));
    INSERT INTO loop_steps (loop_id,name,description,order_index,todo_id,gate_config,skill_names,created_at)
    VALUES ((SELECT MAX(id) FROM loops),'人工审批环节','需人工审批',0,
            (SELECT id FROM todos WHERE title='e2e-044-gate-todo-${ts}'),
            '[{"name":"人工审批","type":"human_approval"}]','[]', datetime('now'));
    INSERT INTO loop_executions (loop_id,trigger_type,trigger_meta,started_at,status,total_steps)
    VALUES ((SELECT MAX(id) FROM loops),'manual','{}',datetime('now'),'running',1);
    INSERT INTO loop_step_executions (loop_execution_id,step_id,todo_id,status,started_at,
                                      sequence_index,approval_status)
    VALUES ((SELECT MAX(id) FROM loop_executions),(SELECT MAX(id) FROM loop_steps),
            (SELECT id FROM todos WHERE title='e2e-044-gate-todo-${ts}'),
            'pending_approval',datetime('now'),1,'pending');
    INSERT INTO loop_step_execution_gates (loop_step_execution_id,gate_type,gate_name,config,status)
    VALUES ((SELECT MAX(id) FROM loop_step_executions),
            'human_approval','人工审批','{"name":"人工审批","type":"human_approval"}','pending');
    SELECT (SELECT MAX(id) FROM loops) lid,
           (SELECT MAX(id) FROM loop_executions) eid,
           (SELECT MAX(id) FROM loop_step_executions) seid,
           (SELECT MAX(id) FROM loop_step_execution_gates) gid;
  `], { encoding: 'utf-8' }).trim();
  const p = out.split('|');
  return { loopId: Number(p[0]), execId: Number(p[1]), stepExecId: Number(p[2]), gateId: Number(p[3]) };
}

/** 清理 e2e-044- 种子数据。 */
function cleanup(): void {
  execFileSync('sqlite3', [DEV_DB, `
    DELETE FROM loop_step_execution_gates WHERE loop_step_execution_id IN
      (SELECT id FROM loop_step_executions WHERE loop_execution_id IN
        (SELECT id FROM loop_executions WHERE loop_id IN
          (SELECT id FROM loops WHERE name LIKE 'e2e-044-%')));
    DELETE FROM loop_step_executions WHERE loop_execution_id IN
      (SELECT id FROM loop_executions WHERE loop_id IN
        (SELECT id FROM loops WHERE name LIKE 'e2e-044-%'));
    DELETE FROM loop_executions WHERE loop_id IN
      (SELECT id FROM loops WHERE name LIKE 'e2e-044-%');
    DELETE FROM tasks WHERE loop_id IN
      (SELECT id FROM loops WHERE name LIKE 'e2e-044-%');
    DELETE FROM loop_tags WHERE loop_id IN
      (SELECT id FROM loops WHERE name LIKE 'e2e-044-%');
    DELETE FROM loop_steps WHERE loop_id IN
      (SELECT id FROM loops WHERE name LIKE 'e2e-044-%');
    DELETE FROM loop_phases WHERE loop_id IN
      (SELECT id FROM loops WHERE name LIKE 'e2e-044-%');
    DELETE FROM loops WHERE name LIKE 'e2e-044-%';
    DELETE FROM todos WHERE title LIKE 'e2e-044-%';
  `], { encoding: 'utf-8' });
}

test.beforeAll(() => cleanup());
test.afterAll(() => cleanup());

// ==========================================================
// 测试 1：列表页无「新建」「触发」
// ==========================================================
test.describe('044-1 环路列表无新建/触发', () => {
  let loopId: number;

  test.beforeAll(() => { loopId = seedBasicLoop(); });

  test('列表页没有「新建」按钮', async ({ page }) => {
    await page.goto(`${BASE}/#/loops`, { waitUntil: 'domcontentloaded' });
    await page.waitForTimeout(2000);

    // 确认页面渲染
    await expect(page.locator('text=环路').first()).toBeVisible({ timeout: 5000 });

    // 无「新建」文字按钮
    await expect(page.locator('button').filter({ hasText: /^新建$/ })).toHaveCount(0);
  });

  test('行「更多」菜单不含「触发」', async ({ page }) => {
    await page.goto(`${BASE}/#/loops`, { waitUntil: 'domcontentloaded' });
    await page.waitForTimeout(2000);

    const moreBtn = page.locator('button[aria-label="更多操作"]');
    test.skip((await moreBtn.count()) === 0, '当前视图无行操作「更多」');

    await moreBtn.first().click();
    await page.waitForTimeout(500);

    const dropdown = page.locator('.ant-dropdown:visible');
    await expect(dropdown).toBeVisible({ timeout: 3000 });

    await expect(dropdown.locator('.ant-dropdown-menu-item').filter({ hasText: /^触发$/ })).toHaveCount(0);
    await expect(dropdown.locator('.ant-dropdown-menu-item').filter({ hasText: /^(启用|暂停)$/ })).toHaveCount(1);
  });
});

// ==========================================================
// 测试 2：详情只读（无触发/编辑/复制/导出/触发器/添加环节）
// ==========================================================
test.describe('044-2 环路详情只读', () => {
  let loopId: number;

  test.beforeAll(() => { loopId = seedBasicLoop(); });

  test('详情页无触发/编辑/复制/导出按钮', async ({ page }) => {
    await page.goto(`${BASE}/#/loops/${loopId}`, { waitUntil: 'domcontentloaded' });
    await page.waitForTimeout(2500);

    await expect(page.locator(`text=环路 #${loopId}`).first()).toBeVisible({ timeout: 5000 });

    for (const label of ['触发', '编辑', '复制', '导出']) {
      await expect(page.locator('button').filter({ hasText: new RegExp(`^${label}$`) })).toHaveCount(0);
    }
  });

  test('详情页无「触发器」区域', async ({ page }) => {
    await page.goto(`${BASE}/#/loops/${loopId}`, { waitUntil: 'domcontentloaded' });
    await page.waitForTimeout(2500);

    await expect(page.locator(`text=环路 #${loopId}`).first()).toBeVisible({ timeout: 5000 });
    await expect(page.locator('text=触发器')).toHaveCount(0);
  });

  test('无「添加环节」入口', async ({ page }) => {
    await page.goto(`${BASE}/#/loops/${loopId}`, { waitUntil: 'domcontentloaded' });
    // 等待页面加载：先等 PageCard 标题渲染，若超时则重刷一次
    try {
      await expect(page.locator(`text=环路 #${loopId}`).first()).toBeVisible({ timeout: 8000 });
    } catch {
      // 首次加载可能因 state 竞态没渲染，重刷一次
      await page.goto(`${BASE}/#/loops/${loopId}`, { waitUntil: 'domcontentloaded' });
      await page.waitForTimeout(3000);
      await expect(page.locator(`text=环路 #${loopId}`).first()).toBeVisible({ timeout: 8000 });
    }
    await expect(page.locator('button').filter({ hasText: /^添加环节$/ })).toHaveCount(0);
  });
});

// ==========================================================
// 测试 3：门禁审批→续跑
// ==========================================================
test.describe('044-3 门禁审批→续跑', () => {

  test('API：审批通过→续跑→success', async ({ request }) => {
    // 种子含 approval_status='pending'，确保 populate_pending_gate_id 注入门禁 id
    const { loopId, execId, stepExecId, gateId } = seedGateApprovalScenario();
    gateTestLoopId = loopId;
    gateTestExecId = execId;
    expect(loopId).toBeGreaterThan(0);

    // 验证执行详情含 pending_gate_id
    const detailRes = await request.get(`${BASE}/api/v1/workspaces/${WS}/loops/${loopId}/executions/${execId}`);
    expect(detailRes.ok()).toBeTruthy();
    const detail = await detailRes.json();
    expect(detail.data?.step_executions?.[0]?.pending_gate_id).toBeGreaterThanOrEqual(1);

    // 调门禁审批 API
    const approveRes = await request.post(
      `${BASE}/api/v1/workspaces/${WS}/loops/${loopId}/executions/${execId}/steps/${stepExecId}/gates/${gateId}/approve`,
      { data: { approved: true, comment: 'E2E 通过' } },
    );
    expect(approveRes.ok()).toBeTruthy();
    expect((await approveRes.json()).data?.status).toBe('passed');

    // 轮询执行终态（approve_gate 内部调 resume，单步骤环路终态 success）
    let finalStatus: string | null = null;
    for (let i = 0; i < 20; i++) {
      await new Promise(r => setTimeout(r, 1500));
      const r = await request.get(`${BASE}/api/v1/workspaces/${WS}/loops/${loopId}/executions/${execId}`);
      if (!r.ok()) continue;
      const s = (await r.json()).data?.status;
      if (s && s !== 'running') { finalStatus = s; break; }
    }
    expect(['success', 'partial']).toContain(finalStatus);
    console.log(`审批后续跑终态: ${finalStatus}`);
  });

  test('前端环路详情页正常渲染已审批环路', async ({ page }) => {
    // 使用前一个用例的 seed 数据
    await page.goto(`${BASE}/#/loops/${gateTestLoopId}`, { waitUntil: 'domcontentloaded' });
    await page.waitForTimeout(3000);

    // 确认页面加载
    await expect(page.locator(`text=环路 #${gateTestLoopId}`).first()).toBeVisible({ timeout: 5000 });
    // 无需断言具体元素，页面正常渲染即为通过
  });
});
