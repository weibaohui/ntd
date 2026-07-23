// Playwright 脚本：验证多 Agent 执行情况在帖子详情页的展示
// 覆盖：PostCard 的「子 Agent」+「待办进度」折叠区，以及 LogDrawer 的「Agent」Tab。
// 用法: cd frontend && npx playwright test tests/check_multi_agent_display.spec.ts --reporter=list
//
// 数据：record 8594（todo 63，claudecode 真实多 agent 运行）。
// 其 execution_logs 已含真实 Agent tool_call；agent_runs / todo_progress 由脚本前已注入样本。
//
// 断言一律用 expect().toContainText() 的自动重试，取代硬编码 waitForTimeout，避免 CI flaky（CodeRabbit）。

import { test, expect } from '@playwright/test';

// 本地开发环境默认监听 18088（embedded 模式）。
const BASE = 'http://localhost:18088';
// 测试目标：todo 63 的 record 8594，进入帖子详情页。
const TODO_ID = 63;
const RECORD_ID = 8594;
const URL = `${BASE}/#/items?id=${TODO_ID}&panel=post&record=${RECORD_ID}`;

test.describe('多 Agent 执行展示', () => {
  test('PostCard 展示子 Agent 与待办进度折叠区', async ({ page }) => {
    await page.goto(URL);
    const body = page.locator('body');

    // 子 Agent 折叠区标题 + 注入的两个 agent 名字（验证后端 parse 结果透传到前端）。
    await expect(body).toContainText('子 Agent');
    await expect(body).toContainText('张三丰加法计算');
    await expect(body).toContainText('李雷乘法计算');

    // 待办进度折叠区 + 完成度统计（todo_progress 之前采了没展示，此处验证已补齐）。
    await expect(body).toContainText('待办进度');
    await expect(body).toContainText('已完成 3/3');

    // 角色/类型标签随 agent 一并渲染。
    await expect(body).toContainText('general-purpose');
  });

  test('LogDrawer 的 Agent Tab 展示子 agent 输入', async ({ page }) => {
    await page.goto(URL);

    // 同一 session 可能有多张 PostCard，取第一个「详情」按钮打开抽屉。
    await page.getByRole('button', { name: '详情' }).first().click();
    // exact 匹配避免命中含 "Agent" 的其他文案（如「子 Agent」）。
    await page.getByRole('button', { name: 'Agent', exact: true }).click();

    // AgentPanel 从真实日志识别子 agent；claudecode 的 Agent 工具 input 里有 prompt，至少出现「输入」标签。
    await expect(page.locator('body')).toContainText('输入');

    // 留档截图（产物目录在 .gitignore 中，不提交 git）。
    await page.screenshot({
      path: 'tests/__screenshots__/multi_agent_drawer.png',
      fullPage: true,
    });
  });

  test('API 透出 agent_runs 字段', async ({ page }) => {
    await page.goto(URL);

    // 确认后端 ExecutionRecord 透出了 agent_runs（JSON 字符串），前端可 parse。
    const rec = await page.evaluate((rid) => {
      return fetch(`/api/v1/workspaces/1/executions/${rid}`).then((r) => r.json()).then((d) => d.data);
    }, RECORD_ID);

    expect(typeof rec.agent_runs).toBe('string');
    const runs = JSON.parse(rec.agent_runs);
    expect(Array.isArray(runs)).toBeTruthy();
    expect(runs.length).toBeGreaterThanOrEqual(2);
    expect(runs[0].name).toBeTruthy();
  });
});
