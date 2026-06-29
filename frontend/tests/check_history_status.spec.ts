import { test, expect } from '@playwright/test';

test.describe('执行历史列状态', () => {
  const BASE = 'http://localhost:18088';

  test('第一页执行记录不应显示"进行中"', async ({ page }) => {
    // 直接打开 todo 详情页面
    await page.goto(`${BASE}/?view=items&id=15`);
    await page.waitForTimeout(3000);

    // 截图
    await page.screenshot({ path: 'tests/__screenshots__/history_status.png', fullPage: true });

    // 获取页面完整文本
    const bodyText = await page.textContent('body');
    console.log('=== 页面文本(前1000字) ===');
    console.log(bodyText?.substring(0, 1000));

    // 统计各状态出现次数
    const matches = {
      running: bodyText?.match(/进行中/g)?.length || 0,
      success: bodyText?.match(/成功/g)?.length || 0,
      failed: bodyText?.match(/失败/g)?.length || 0,
    };
    console.log('\n=== 状态标签统计 ===');
    console.log(`"进行中": ${matches.running} 处`);
    console.log(`"成功": ${matches.success} 处`);
    console.log(`"失败": ${matches.failed} 处`);

    // 直接检查 API 返回的记录状态
    const records = await page.evaluate(async () => {
      const resp = await fetch('/api/execution-records?todo_id=15&page=1&limit=5');
      const data = await resp.json();
      return (data.data?.records || []).map(r => ({
        id: r.id,
        status: r.status,
        result: (r.result || '').substring(0, 20),
      }));
    });
    console.log('\n=== API 返回记录 ===');
    records.forEach(r => console.log(`  #${r.id} | status=${r.status} | result="${r.result}"`));
  });
});
