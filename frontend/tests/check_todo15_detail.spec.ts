import { test, expect } from '@playwright/test';

test.describe('Todo #15 详情面板', () => {
  const TODO_ID = 15;
  const BASE = 'http://localhost:18088';

  test('执行历史面板状态显示', async ({ page }) => {
    // 1. 打开页面
    await page.goto(`${BASE}/?view=items&id=${TODO_ID}`);
    await page.waitForTimeout(3000);

    // 截屏
    await page.screenshot({ path: 'tests/__screenshots__/todo15_initial.png', fullPage: true });

    // 2. 尝试点击 todo #15 的卡片
    // 从页面中找到包含 "工具测试" 的卡片并点击
    const todoCard = page.locator('text=工具测试').first();
    if (await todoCard.isVisible()) {
      await todoCard.click();
      await page.waitForTimeout(2000);
      
      await page.screenshot({ path: 'tests/__screenshots__/todo15_clicked.png', fullPage: true });
      
      // 获取右侧面板文本
      const panelText = await page.textContent('body');
      console.log('=== 点击后的页面文本 ===');
      console.log(panelText?.substring(0, 3000));
    } else {
      console.log('Todo #15 未在页面上可见');
    }
  });
});
