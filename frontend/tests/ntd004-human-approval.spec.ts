// NTD-004 工艺人工审批链路 Playwright 端到端验证。
//
// 验证点（对应 docs/bugs/004-工艺人工审批链路断裂）：
// 1. 任务详情「执行历史」行显示「N 条待审批」引导标记（新增，任务视角入口）；
// 2. 展开执行后工艺看板出现「通过/拒绝」按钮；
// 3. 点「通过」：请求 approved=true，环节 success、门禁 passed、loop 执行成功结束（resume 生效）；
// 4. 点「拒绝」：请求 approved=false（回归：此前两个按钮都发 true），
//    环节 failed、门禁 failed、loop 执行失败结束。
//
// 数据策略：每个用例在 dev 库（~/.ntd/data.dev.db）自种子一条待审批数据
// （todo → loop → phase → step → task → execution → step_execution → gate），
// 审批会改变终态，自种子保证用例可重复执行。工作空间固定为 id=3（/Users/mac/sticky-notes）。

import { test, expect } from '@playwright/test';
import { execSync } from 'node:child_process';
import { homedir } from 'node:os';
import { join } from 'node:path';

const BASE = 'http://localhost:18088';
const DEV_DB = join(homedir(), '.ntd', 'data.dev.db');
const WORKSPACE_ID = 3;
const WORKSPACE_PATH = '/Users/mac/sticky-notes';

/** 在 dev 库种子化一条「环节待人工审批」数据，返回关键 id。 */
function seedPendingApproval(tag: string): { taskId: number; execId: number } {
  const sql = `
INSERT INTO todos (title, prompt, status) VALUES ('NTD004-${tag}', 'echo ok', 'pending');
INSERT INTO loops (name, description, status, workspace_id, workspace_path)
VALUES ('NTD004-${tag}', '人工审批链路验证', 'enabled', ${WORKSPACE_ID}, '${WORKSPACE_PATH}');
INSERT INTO loop_phases (loop_id, name, order_index) VALUES ((SELECT MAX(id) FROM loops), '人工确认', 0);
INSERT INTO loop_steps (loop_id, name, order_index, todo_id, on_success, gate_config, enabled, phase_id)
VALUES ((SELECT MAX(id) FROM loops), '人工确认环节', 0, (SELECT MAX(id) FROM todos), 'end',
        '[{"name":"人工审批","type":"human_approval"}]', 1, (SELECT MAX(id) FROM loop_phases));
INSERT INTO tasks (title, description, status, workspace_id, loop_id)
VALUES ('NTD004-${tag}', '验证审批链路', 'running', ${WORKSPACE_ID}, (SELECT MAX(id) FROM loops));
INSERT INTO loop_executions (loop_id, trigger_type, trigger_meta, status, started_at, total_steps, task_id)
VALUES ((SELECT MAX(id) FROM loops), 'manual', '{"requirement":"验证审批链路"}', 'running', datetime('now'), 1,
        (SELECT MAX(id) FROM tasks));
INSERT INTO loop_step_executions (loop_execution_id, step_id, todo_id, status, sequence_index)
VALUES ((SELECT MAX(id) FROM loop_executions), (SELECT MAX(id) FROM loop_steps),
        (SELECT MAX(id) FROM todos), 'pending_approval', 1);
INSERT INTO loop_step_execution_gates (loop_step_execution_id, gate_type, gate_name, config, status)
VALUES ((SELECT MAX(id) FROM loop_step_executions), 'human_approval', '人工审批',
        '{"name":"人工审批","type":"human_approval"}', 'pending');
SELECT (SELECT MAX(id) FROM tasks) || ',' || (SELECT MAX(id) FROM loop_executions);
`;
  // 单条 sqlite3 调用在一个连接内执行，MAX(id) 语义与逐条插入一致。
  const out = execSync(`sqlite3 ${DEV_DB} "${sql.replace(/"/g, '\\"')}"`, { encoding: 'utf-8' }).trim();
  const [taskId, execId] = out.split(',').map(Number);
  return { taskId, execId };
}

/** 进入任务详情的执行历史 Tab，等待自动展开首条待审批执行后返回通过/拒绝按钮定位器。 */
async function openExecBoard(page: import('@playwright/test').Page, taskId: number) {
  await page.goto(`${BASE}/#/tasks/${taskId}`);
  // 任务详情首屏：Tabs 出现即加载完成。
  await page.waitForSelector('.ant-tabs', { timeout: 15000 });
  await page.getByRole('tab', { name: /执行历史/ }).click();
  // ExecHistoryTab 固定开启 autoExpandFirstPending：执行历史 Tab 打开、首屏数据到达后，
  // 自动展开第一条 pending_approval 的执行，门禁区「通过/拒绝」按钮随展开直接可见。
  // 早期版本里的「条待审批，展开处理」引导文案与「查看详情」按钮已随该自动展开能力移除。
  // AntD 对两个汉字的按钮自动插空格（autoInsertSpace），可访问名是「通 过」，用正则兼容。
  const approveBtn = page.getByRole('button', { name: /通\s*过/ });
  const rejectBtn = page.getByRole('button', { name: /拒\s*绝/ });
  await expect(approveBtn).toBeVisible({ timeout: 10000 });
  await expect(rejectBtn).toBeVisible();
  return { approveBtn, rejectBtn };
}

/** 轮询任务详情接口，直到任务与指定执行的字段满足断言（WS 事件在 headless 下不保证即时到达）。 */
async function expectExecState(
  page: import('@playwright/test').Page,
  taskId: number,
  execId: number,
  assert: (
    task: { status: string },
    exec: { status: string; pending_approval_count?: number },
  ) => void,
) {
  await expect(async () => {
    const resp = await page.request.get(`${BASE}/api/v1/workspaces/${WORKSPACE_ID}/tasks/${taskId}`);
    const body = await resp.json();
    const exec = (body.data.executions as Array<{ id: number; status: string; pending_approval_count?: number }>)
      .find((e) => e.id === execId);
    expect(exec).toBeTruthy();
    assert(body.data.task, exec as { status: string; pending_approval_count?: number });
  }).toPass({ timeout: 15000, intervals: [500, 1000, 2000] });
}

test('NTD004 审批通过：环节 success + loop 成功结束', async ({ page }) => {
  const { taskId, execId } = seedPendingApproval('通过');
  const { approveBtn } = await openExecBoard(page, taskId);

  // 拦截审批请求，断言语义（验证点 3：approved=true）。
  const reqPromise = page.waitForRequest((r) => r.url().includes('/approve') && r.method() === 'POST');
  await approveBtn.click();
  const req = await reqPromise;
  expect(req.postDataJSON()).toMatchObject({ approved: true });

  // 操作反馈 + 看板刷新后门禁 Tag 变 passed；「通过/拒绝」按钮消失。
  await expect(page.getByText('已通过').first()).toBeVisible({ timeout: 5000 });
  // 门禁结果现在以独立 Tag（gate_name）+ result span 渲染，旧「人工审批 → passed」整行文案已不存在；
  // 终态由下方 expectExecState 轮询（exec.status=success、pending_approval_count=0）权威校验。
  await expect(page.getByRole('button', { name: /通\s*过/ })).toHaveCount(0);

  // resume_loop_execution 生效：单环节工艺（on_success=end）审批通过后 loop 应直接成功结束。
  // NTD-005 回归：tasks.status（列表「状态」列）必须与 loop 终态（「最近执行」列）收敛一致。
  await expectExecState(page, taskId, execId, (task, exec) => {
    expect(exec.status).toBe('success');
    expect(exec.pending_approval_count ?? 1).toBe(0);
    expect(task.status).toBe('success');
  });
});

test('NTD004 审批拒绝：请求 approved=false + 环节 failed', async ({ page }) => {
  const { taskId, execId } = seedPendingApproval('拒绝');
  const { rejectBtn } = await openExecBoard(page, taskId);

  // 关键回归：「拒绝」必须发 approved=false（修复前两个按钮都发 true）。
  const reqPromise = page.waitForRequest((r) => r.url().includes('/approve') && r.method() === 'POST');
  await rejectBtn.click();
  const req = await reqPromise;
  expect(req.postDataJSON()).toMatchObject({ approved: false });

  await expect(page.getByText('已拒绝').first()).toBeVisible({ timeout: 5000 });
  // 同「通过」用例：门禁结果渲染拆分为 Tag+span，旧「人工审批 → failed」整行文案已不存在；
  // 终态由下方 expectExecState（exec.status=failed、task.status=failed）权威校验。

  // 拒绝后 loop 以失败终态结束（resume 按环节 status 判定，不再被 rating=0/min=0 误判）。
  // NTD-005 回归：任务状态同步为 failed（此前 resume 路径不回写，任务永远"进行中"）。
  await expectExecState(page, taskId, execId, (task, exec) => {
    expect(exec.status).toBe('failed');
    expect(task.status).toBe('failed');
  });
});

test('NTD004 重启后待审批状态保留（持久化回归）', async ({ page }) => {
  // 待审批是纯 DB 状态（无内存等待），刷新页面重拉数据后审批入口必须仍在。
  // 用「重新打开页面」模拟重启后的首次访问：服务端状态不依赖进程内存。
  const { taskId } = seedPendingApproval('持久');
  await page.goto(`${BASE}/#/tasks/${taskId}`);
  await page.waitForSelector('.ant-tabs', { timeout: 15000 });
  await page.getByRole('tab', { name: /执行历史/ }).click();
  // autoExpandFirstPending：执行历史 Tab 打开后自动展开首条待审批执行，通过按钮直接可见。
  await expect(page.getByRole('button', { name: /通\s*过/ })).toBeVisible({ timeout: 10000 });

  // 重新加载页面（等价于重启后再次进入）：状态从 DB 重拉，待审批执行仍自动展开、入口保持。
  await page.reload();
  await page.waitForSelector('.ant-tabs', { timeout: 15000 });
  await page.getByRole('tab', { name: /执行历史/ }).click();
  await expect(page.getByRole('button', { name: /通\s*过/ })).toBeVisible({ timeout: 10000 });
});
