// Playwright 脚本：检查执行记录详情页的状态和结论显示
// 用法: cd frontend && npx playwright test tests/check_execution_record.spec.ts

import { test, expect } from '@playwright/test';

test.describe('执行记录详情页', () => {
  // 记录 5883: kilo 执行 date+whoami，已完成但 result 为空
  const RECORD_ID = 5883;
  const TODO_ID = 15;
  const BASE = 'http://localhost:18088';
  const URL = `${BASE}/?view=items&id=${TODO_ID}&panel=post&record=${RECORD_ID}`;

  test('应显示成功状态而非进行中', async ({ page }) => {
    await page.goto(URL);
    await page.waitForTimeout(2000); // 等待页面加载

    // 1. 检查 API 返回的状态
    const apiResponse = await page.evaluate((recordId) => {
      return fetch(`/api/execution-records/${recordId}`)
        .then(r => r.json())
        .then(data => data.data);
    }, RECORD_ID);
    console.log('API 返回:', JSON.stringify({ status: apiResponse.status, result: apiResponse.result, finished_at: apiResponse.finished_at }));
    expect(apiResponse.status).toBe('success');

    // 2. 检查页面中状态标签的文本
    // 状态标签在有 status=success 的 span 附近
    const pageText = await page.textContent('body');
    console.log('页面文本:', pageText?.substring(0, 500));

    // 3. 检查是否有"进行中"字样
    const hasRunningText = pageText?.includes('进行中');
    console.log('包含"进行中":', hasRunningText);

    // 4. 检查是否有"成功"或"暂无结论"
    const hasSuccessText = pageText?.includes('成功');
    const hasNoResultText = pageText?.includes('暂无结论');
    console.log('包含"成功":', hasSuccessText);
    console.log('包含"暂无结论":', hasNoResultText);

    // 断言：不应显示"进行中"
    expect(hasRunningText).toBeFalsy();
  });
});
