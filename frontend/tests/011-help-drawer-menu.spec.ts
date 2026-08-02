// NTD-011 回归验证：帮助抽屉子菜单点击无响应。
//
// 验证点：
// 1. 打开帮助抽屉后，右侧内容区渲染出当前页面总览 md（含标题元素），不再空白。
// 2. 点击父节点「标题文字」即可展开子菜单（修复前必须点 switcher 小箭头）。
// 3. 点击功能点子节点后，右侧内容切换为对应 md，且 console 无 X-Markdown 类型错误。
import { test, expect } from '@playwright/test';

// 读取右侧内容区（flex 容器第二个子元素）的渲染信息
async function readContentInfo(page: import('@playwright/test').Page) {
  return page.evaluate(() => {
    const body = document.querySelector('.ant-drawer-open .ant-drawer-body');
    const contentDiv = body?.firstElementChild?.children[1];
    return {
      text: contentDiv?.textContent?.slice(0, 300) ?? null,
      headings: Array.from(contentDiv?.querySelectorAll('h1,h2,h3') ?? []).map(h => h.textContent),
    };
  });
}

test('NTD-011 帮助抽屉子菜单可点开且内容渲染', async ({ page }) => {
  // 捕获浏览器 console 错误：修复前 XMarkdown 会报「input must be string, not function」
  const consoleErrors: string[] = [];
  page.on('console', msg => { if (msg.type() === 'error') consoleErrors.push(msg.text()); });
  page.on('pageerror', err => consoleErrors.push(`pageerror: ${err.message}`));

  await page.goto('http://localhost:18088');
  await page.waitForTimeout(2000);

  // 打开帮助抽屉
  await page.click('button[aria-label="帮助"]');
  await expect(page.locator('.ant-drawer-open')).toBeVisible();
  await page.waitForTimeout(800);

  // 验证点 1：默认选中页的总览 md 已渲染（有标题元素，非空白）
  const initial = await readContentInfo(page);
  console.log('初始内容标题:', JSON.stringify(initial.headings));
  expect(initial.headings.length).toBeGreaterThan(0);

  // 验证点 2：点击父节点「标题文字」，子菜单应展开（出现功能点子节点）
  await page.locator('.ant-drawer-open .ant-tree-title', { hasText: '仪表盘' }).first().click();
  await page.waitForTimeout(400);
  const childTitle = page.locator('.ant-drawer-open .ant-tree-title', { hasText: 'Tab 切换' });
  await expect(childTitle.first()).toBeVisible();

  // 验证点 3：点击功能点子节点，右侧内容切换为对应 md
  await childTitle.first().click();
  await page.waitForTimeout(500);
  const feature = await readContentInfo(page);
  console.log('子节点内容标题:', JSON.stringify(feature.headings));
  expect(feature.headings.length).toBeGreaterThan(0);
  expect(feature.text).toContain('Tab');

  // 全程不应出现 X-Markdown 输入类型错误
  const xmdErrors = consoleErrors.filter(e => e.includes('X-Markdown'));
  console.log('X-Markdown 错误数:', xmdErrors.length);
  expect(xmdErrors).toHaveLength(0);
});
