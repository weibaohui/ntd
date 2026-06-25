// 验证 Loop 异常处理 Todo 功能是否正常启用
import { test, expect, chromium } from '@playwright/test';

test('Loop 异常处理 Todo 功能验证', async () => {
  const browser = await chromium.launch({ headless: true });
  const context = await browser.newContext();
  const page = await context.newPage();

  // 打开应用
  await page.goto('http://localhost:18088');
  await page.waitForTimeout(3000);

  // 截图初始状态
  await page.screenshot({ path: 'tests/__screenshots__/abnormal_handler_01_initial.png' });

  // 打印页面 title
  console.log('页面标题:', await page.title());

  // 查找所有文本内容来理解页面结构
  const bodyText = await page.locator('body').innerText();
  console.log('页面内容前500字符:', bodyText.substring(0, 500));

  // 尝试查找 Loop 相关的入口
  // 可能是在侧边栏或者其他位置
  const allLinks = await page.locator('a').all();
  console.log('页面链接数量:', allLinks.length);

  // 尝试查找点击进入 Loop 页面的按钮
  // 可能是 tab 或者 sidebar item
  const loopTab = page.locator('text=/loop|Loop|环路/i').first();
  if (await loopTab.isVisible()) {
    console.log('找到 Loop 入口');
    await loopTab.click();
    await page.waitForTimeout(2000);
    await page.screenshot({ path: 'tests/__screenshots__/abnormal_handler_02_loop_page.png' });
  } else {
    console.log('未找到 Loop 入口，检查是否有 Tab');
    // 打印所有可见的 tab 或者 sidebar item
    const sidebarItems = await page.locator('[class*="sidebar"], [class*="menu"], [class*="nav"]').all();
    console.log('sidebar/menu/nav 元素数量:', sidebarItems.length);
  }

  // 尝试直接打开 Loop Studio 页面
  await page.goto('http://localhost:18088/#/loops');
  await page.waitForTimeout(2000);
  await page.screenshot({ path: 'tests/__screenshots__/abnormal_handler_03_loops_page.png' });

  const loopsPageText = await page.locator('body').innerText();
  console.log('Loops 页面内容前500字符:', loopsPageText.substring(0, 500));

  // 检查是否有 新建/创建 按钮
  const createBtn = page.locator('button:has-text("新建"), button:has-text("创建"), button:has-text("+")').first();
  if (await createBtn.isVisible()) {
    console.log('找到创建按钮');
    await createBtn.click();
    await page.waitForTimeout(1000);
    await page.screenshot({ path: 'tests/__screenshots__/abnormal_handler_04_modal.png' });

    // 检查异常处理区块
    const abnormalSection = page.locator('text=异常处理').first();
    console.log('异常处理区块可见:', await abnormalSection.isVisible().catch(() => false));

    const abnormalTodoSelector = page.locator('text=异常处理 Todo').first();
    console.log('异常处理 Todo 选择器可见:', await abnormalTodoSelector.isVisible().catch(() => false));

    const triggerCondition = page.locator('text=触发条件').first();
    console.log('触发条件可见:', await triggerCondition.isVisible().catch(() => false));
  }

  await browser.close();
});
