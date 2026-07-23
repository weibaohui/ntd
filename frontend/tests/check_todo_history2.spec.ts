import { test, expect } from '@playwright/test';

test.describe('Todo #15 执行记录详情', () => {
  const TODO_ID = 15;
  const BASE = 'http://localhost:18088';

  test('检查执行历史状态显示', async ({ page }) => {
    // 1. 打开 todo 详情页
    await page.goto(`${BASE}/?view=items&id=${TODO_ID}`);
    await page.waitForTimeout(4000);

    // 截取完整页面截图
    await page.screenshot({ path: 'tests/__screenshots__/todo15_full.png', fullPage: true });

    // 2. 获取页面完整文本
    const bodyText = await page.textContent('body');
    console.log('=== 页面完整文本 ===');
    console.log(bodyText);

    // 3. 检查关键文本
    console.log('\n=== 状态检查 ===');
    const hasRunning = bodyText?.includes('进行中');
    const hasSuccess = bodyText?.includes('成功');
    const hasFail = bodyText?.includes('失败');
    const hasNoResult = bodyText?.includes('暂无结论');
    console.log(`"进行中": ${hasRunning}`);
    console.log(`"成功": ${hasSuccess}`);
    console.log(`"失败": ${hasFail}`);
    console.log(`"暂无结论": ${hasNoResult}`);

    // 4. 检查 API 返回的最新执行记录
    const records = await page.evaluate(async (todoId) => {
      const resp = await fetch(`/api/v1/workspaces/1/executions?todo_id=${todoId}&page=1&limit=5`);
      const data = await resp.json();
      return data.data?.records || [];
    }, TODO_ID);
    console.log('\n=== 最新执行记录 ===');
    for (const r of records.slice(0, 3)) {
      console.log(`#${r.id} | ${r.executor} | status=${r.status} | result="${(r.result || '').substring(0, 60)}"`);
    }
  });
});
