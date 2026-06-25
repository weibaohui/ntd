// 触发 Loop 执行并观察效果
import { test, expect, chromium } from '@playwright/test';

test('触发 Loop 执行并观察异常处理', async () => {
  const browser = await chromium.launch({ headless: true });
  const context = await browser.newContext();
  const page = await context.newPage();

  // 打开 Loop 页面
  await page.goto('http://localhost:18088/#/loops');
  await page.waitForTimeout(2000);

  // 截图初始状态
  await page.screenshot({ path: 'tests/__screenshots__/trigger_01_loops_list.png' });

  // 找一个已启用的 Loop，比如 #20 "ee"
  // 点击进入 Loop 详情
  const loopRow = page.locator('text=ee').first();
  if (await loopRow.isVisible()) {
    console.log('找到 Loop #20 ee');
    await loopRow.click();
    await page.waitForTimeout(2000);
    await page.screenshot({ path: 'tests/__screenshots__/trigger_02_loop_detail.png' });

    // 找触发按钮
    const triggerBtn = page.locator('button:has-text("触发"), button:has-text("执行"), button:has-text("运行")').first();
    if (await triggerBtn.isVisible()) {
      console.log('找到触发按钮，点击执行');
      await triggerBtn.click();
      await page.waitForTimeout(3000);
      await page.screenshot({ path: 'tests/__screenshots__/trigger_03_after_trigger.png' });

      // 等待一段时间让 Loop 执行
      console.log('等待 Loop 执行...');
      await page.waitForTimeout(10000);
      await page.screenshot({ path: 'tests/__screenshots__/trigger_04_executing.png' });

      // 检查执行历史
      const executionsSection = page.locator('text=执行历史,历史,执行记录').first();
      if (await executionsSection.isVisible().catch(() => false)) {
        await executionsSection.click();
        await page.waitForTimeout(1000);
        await page.screenshot({ path: 'tests/__screenshots__/trigger_05_execution_history.png' });
      }
    } else {
      console.log('没有找到触发按钮');
    }
  } else {
    // 尝试点击任何一个 Loop
    console.log('没有找到 ee，尝试点击其他 Loop');
    const anyLoop = page.locator('[class*="loop"], [class*="card"]').first();
    if (await anyLoop.isVisible()) {
      await anyLoop.click();
      await page.waitForTimeout(2000);
      await page.screenshot({ path: 'tests/__screenshots__/trigger_02_any_loop.png' });
    }
  }

  console.log('测试完成');
  await browser.close();
});
