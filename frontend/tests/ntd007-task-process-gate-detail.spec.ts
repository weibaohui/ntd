import { test, expect } from '@playwright/test';
import { execFileSync } from 'node:child_process';
import { homedir } from 'node:os';
import { join } from 'node:path';

const BASE = 'http://localhost:18088';
const DEV_DB = join(homedir(), '.ntd', 'data.dev.db');
const WORKSPACE_ID = 1;

// 自种子一条带 AI 评审门禁的任务详情数据：gate_config.min_score 是本用例的核心输入。
function seedTaskWithAiGate(tag: string): { taskId: number; title: string } {
  const title = `NTD007-${tag}-${Date.now()}`;
  const sql = `
INSERT INTO todos (title, prompt, status, executor, workspace_id, workspace_path, created_at, updated_at)
VALUES ('${title}-todo', '测试工艺要求门禁展示', 'pending', 'claudecode', ${WORKSPACE_ID}, '/tmp',
        strftime('%Y-%m-%dT%H:%M:%SZ','now','utc'), strftime('%Y-%m-%dT%H:%M:%SZ','now','utc'));
INSERT INTO loops (name, description, workspace_id, workspace_path, status, created_at, updated_at)
VALUES ('${title}-loop', '验证任务详情工艺要求门禁展示', ${WORKSPACE_ID}, '/tmp', 'paused',
        strftime('%Y-%m-%dT%H:%M:%SZ','now','utc'), strftime('%Y-%m-%dT%H:%M:%SZ','now','utc'));
INSERT INTO loop_steps (loop_id, name, description, order_index, todo_id, expected_artifacts, gate_config, skill_names, created_at)
VALUES ((SELECT MAX(id) FROM loops), '生成 PRD', '验证门禁阈值展示', 0, (SELECT MAX(id) FROM todos),
        '[]', '[{"name":"AI 评分达标","type":"ai_criteria_review","min_score":80,"timeout_secs":30}]', '[]',
        strftime('%Y-%m-%dT%H:%M:%SZ','now','utc'));
INSERT INTO tasks (title, description, status, workspace_id, loop_id, created_by, created_at, updated_at)
VALUES ('${title}', '验证任务详情工艺要求门禁阈值展示', 'pending', ${WORKSPACE_ID}, (SELECT MAX(id) FROM loops), 'playwright',
        strftime('%Y-%m-%dT%H:%M:%SZ','now','utc'), strftime('%Y-%m-%dT%H:%M:%SZ','now','utc'));
SELECT MAX(id) FROM tasks;
`;
  const out = execFileSync('sqlite3', [DEV_DB, sql], { encoding: 'utf-8' }).trim();
  return { taskId: Number(out), title };
}

// 打开任务详情前固定亮色主题：本用例只验证工艺要求 tab 的门禁信息完整性，
// 主题在这里不是变量，固定亮色可以减少断言噪音。
async function openTaskDetail(page: import('@playwright/test').Page, taskId: number) {
  await page.addInitScript(() => {
    localStorage.setItem('app_theme', 'light');
    localStorage.setItem('ntd_tasks_view', 'list');
    // 任务自种子到 ws1，但 SPA 默认锁 dirs[0]（dev 库首项是 ws3）；不钉 ws1 会拉不到该任务，
    // 详情面板不渲染，门禁所在 tab 无从出现。
    localStorage.setItem('selected_workspace', '1');
  });
  await page.goto(`${BASE}/#/tasks/${taskId}`, { waitUntil: 'domcontentloaded' });
  // 093 重构后任务详情 tab 为：概览 / 执行环路 / 执行历史 / 讨论；旧的「工艺要求」并入「执行环路」
  //（DAGTab），门禁在该 tab 的「步骤验收标准」区渲染（见 TaskDetailTabs.gateDetailText）。
  await expect(page.getByRole('tab', { name: /执行环路/ })).toBeVisible();
}

test('NTD-007：工艺要求 tab 的 AI 评审门禁展示通过阈值', async ({ page }) => {
  const { taskId } = seedTaskWithAiGate('阈值');
  await openTaskDetail(page, taskId);

  // 切到「执行环路」tab（原「工艺要求」，093 重构后并入）：门禁配置在步骤验收标准区渲染，执行历史 tab 不参与本断言。
  await page.getByRole('tab', { name: /执行环路/ }).click();
  const activePane = page.locator('.ant-tabs-tabpane-active');

  // 环节名与门禁名先定位，确保断言落在目标步骤而不是页面其他区域。
  // 「执行环路」tab 同时含 DAG 流程图（SVG <text>节点）与「验收标准」步骤列表（<div>），
  // 两者都渲染环节名「生成 PRD」；用 div + 精确文本收窄到步骤列表，避开 SVG 节点导致的 strict-mode 多匹配。
  await expect(activePane.locator('div').filter({ hasText: /^生成 PRD$/ })).toBeVisible();
  // 用 exact：环路描述「验证任务详情工艺要求门禁展示」含「门禁」子串，会与步骤标签 <span>门禁</span> 同时命中。
  await expect(activePane.getByText('门禁', { exact: true })).toBeVisible();
  await expect(activePane.getByText(/AI 评分达标/)).toBeVisible();

  // 核心回归断言：AI 评审门禁必须显示通过阈值与等待超时，不能只显示「AI 评审」类型。
  await expect(activePane.getByText(/AI 评审 · 阈值 ≥ 80 分；等待 ≤ 30s/)).toBeVisible();
});
