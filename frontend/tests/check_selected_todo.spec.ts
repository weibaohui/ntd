import { test, expect } from '@playwright/test';

test.describe('Todo #15 选中状态', () => {
  test('URL id=15 应选中 todo', async ({ page }) => {
    await page.goto('http://localhost:18088/?view=items&id=15');
    
    // 等待初始加载完成（loading 状态结束 + 请求完成）
    await page.waitForTimeout(5000);
    
    // 检查页面是否有选中状态
    const bodyText = await page.textContent('body');
    console.log('=== 页面文本(前1500字) ===');
    console.log(bodyText?.substring(0, 1500));
    
    // 检查 URL 是否正确
    const url = page.url();
    console.log('\n当前 URL:', url);
    
    // 检查 React state
    const selectedState = await page.evaluate(() => {
      // @ts-ignore
      return window.__SELECTED_TODO_ID__ || 'not found';
    });
    console.log('selectedTodoId:', selectedState);
  });
});
