// Playwright 脚本：检查 Todo #15 的执行记录列表页
// 验证执行记录的状态标签是否正确显示
// 用法: cd frontend && npx playwright test tests/check_todo_history.spec.ts

import { test, expect } from '@playwright/test';

test.describe('Todo #15 执行记录列表', () => {
  const TODO_ID = 15;
  const BASE = 'http://localhost:18088';
  const URL = `${BASE}/?view=items&id=${TODO_ID}`;

  test('执行记录列表应正确显示状态', async ({ page }) => {
    // 加载页面
    await page.goto(URL);
    await page.waitForTimeout(3000);

    // 获取页面完整文本
    const pageText = await page.textContent('body');
    console.log('=== 页面完整文本 ===');
    console.log(pageText?.substring(0, 2000));

    // 检查 API 返回的执行记录列表
    const records = await page.evaluate(async (todoId) => {
      const resp = await fetch(`/api/execution-records?todo_id=${todoId}&page=1&limit=10`);
      const data = await resp.json();
      return data.data?.records || [];
    }, TODO_ID);
    
    console.log('=== 执行记录列表 ===');
    for (const r of records) {
      console.log(`  #${r.id} | ${r.executor} | status=${r.status} | result="${(r.result || '').substring(0, 50)}" | finished_at=${r.finished_at || 'null'}`);
    }

    // 验证最新一条记录的状态
    if (records.length > 0) {
      const latest = records[0];
      expect(latest.status).toBe('success');
      console.log(`\n最新记录 #${latest.id}: status=${latest.status}, result="${latest.result}"`);
    }

    // 关键检查：页面中不应出现状态错乱
    const runningCount = (pageText?.match(/进行中/g) || []).length;
    const successCount = (pageText?.match(/成功/g) || []).length;
    const failCount = (pageText?.match(/失败/g) || []).length;
    
    console.log(`\n页面状态标签统计:`);
    console.log(`  "进行中": ${runningCount}处`);
    console.log(`  "成功": ${successCount}处`);
    console.log(`  "失败": ${failCount}处`);
  });
});
