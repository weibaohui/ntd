// Playwright 脚本：验证多 Agent 执行情况在帖子详情页的展示
// 覆盖：PostCard 的「子 Agent」+「待办进度」折叠区，以及 LogDrawer 的「Agent」Tab。
// 用法: cd frontend && npx playwright test tests/check_multi_agent_display.spec.ts --reporter=list
//
// 数据：record 8594（todo 63，claudecode 真实多 agent 运行）。
// 其 execution_logs 已含真实 Agent tool_call；agent_runs / todo_progress 由脚本前已注入样本。

import { test, expect } from '@playwright/test';

const BASE = 'http://localhost:18088';
const TODO_ID = 63;
const RECORD_ID = 8594;
const URL = `${BASE}/#/items?id=${TODO_ID}&panel=post&record=${RECORD_ID}`;

test.describe('多 Agent 执行展示', () => {
  test('PostCard 展示子 Agent 与待办进度折叠区', async ({ page }) => {
    await page.goto(URL);
    await page.waitForTimeout(2500); // 等待 session 记录加载与渲染

    const body = (await page.textContent('body')) ?? '';

    // 1. 子 Agent 折叠区标题 + 注入的两个 agent 名字
    expect(body).toContain('子 Agent');
    expect(body).toContain('张三丰加法计算');
    expect(body).toContain('李雷乘法计算');

    // 2. 待办进度折叠区 + 完成度统计
    expect(body).toContain('待办进度');
    expect(body).toContain('已完成 3/3');

    // 3. 角色标签
    expect(body).toContain('general-purpose');
  });

  test('LogDrawer 的 Agent Tab 展示子 agent 输入输出', async ({ page }) => {
    await page.goto(URL);
    await page.waitForTimeout(2500);

    // 打开详情抽屉（同 session 可能有多张 PostCard，取第一个「详情」按钮）
    await page.getByRole('button', { name: '详情' }).first().click();
    await page.waitForTimeout(1000);

    // 切到 Agent Tab
    await page.getByRole('button', { name: 'Agent', exact: true }).click();
    await page.waitForTimeout(1000);

    const drawer = (await page.textContent('body')) ?? '';

    // AgentPanel 从真实日志识别出子 agent；claudecode 的 Agent 工具 input 里有 prompt。
    // 至少应出现「输入」标签（每个 agent 的 prompt 展示块）。
    expect(drawer).toContain('输入');

    // 留档截图（产物目录，不提交 git）
    await page.screenshot({ path: 'tests/__screenshots__/multi_agent_drawer.png', fullPage: true });
  });

  test('API 透出 agent_runs 字段', async ({ page }) => {
    await page.goto(URL);
    await page.waitForTimeout(1500);

    // 确认后端 ExecutionRecord 透出了 agent_runs（JSON 字符串），前端可 parse。
    const rec = await page.evaluate((rid) => {
      return fetch(`/api/execution-records/${rid}`).then((r) => r.json()).then((d) => d.data);
    }, RECORD_ID);

    expect(typeof rec.agent_runs).toBe('string');
    const runs = JSON.parse(rec.agent_runs);
    expect(Array.isArray(runs)).toBeTruthy();
    expect(runs.length).toBeGreaterThanOrEqual(2);
    expect(runs[0].name).toBeTruthy();
  });
});
