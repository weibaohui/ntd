// NTD-011 回归验证：帮助抽屉子菜单点击无响应。
//
// 验证点：
// 1. 打开帮助抽屉后，右侧内容区渲染出当前页面总览 md（含标题元素），不再空白。
// 2. 点击父节点「标题文字」即可展开子菜单（修复前必须点 switcher 小箭头）。
// 3. 点击功能点子节点后，右侧内容切换为对应 md，且 console 无 X-Markdown 类型错误。
import { test, expect } from '@playwright/test';

// 读取右侧内容区（.ntd-help-content）的渲染信息。
// 帮助已从 antd Drawer 重构为 antd Modal（HelpPage），内容容器类名同步改为 .ntd-help-content，
// 不再有 .ant-drawer-body / 两栏 flex 结构，因此这里直接取内容容器。
async function readContentInfo(page: import('@playwright/test').Page) {
  return page.evaluate(() => {
    const contentDiv = document.querySelector('.ntd-help-content');
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

  // 打开帮助弹窗（HelpPage 已重构为 antd Modal，角色为 dialog）
  await page.click('[data-testid="left-rail-help"]');
  await expect(page.getByRole('dialog')).toBeVisible();
  await page.waitForTimeout(800);

  // 验证点 1：默认选中页的总览 md 已渲染（有标题元素，非空白）。
  // readContentInfo 是一次性 DOM 读取、无自动重试，故先用 expect.poll 等待标题出现，
  // 避免慢速 CI 上读到渲染前的空 DOM（PR #978 评审：关键读取点用状态驱动替代固定等待）。
  await expect.poll(async () => (await readContentInfo(page)).headings.length).toBeGreaterThan(0);
  const initial = await readContentInfo(page);
  console.log('初始内容标题:', JSON.stringify(initial.headings));

  // 验证点 2：点击一级页面节点「仪表盘」标题，子菜单应展开（出现功能点子节点）。
  // 帮助菜单已从 antd Tree 重构为自定义 <button> 列表：页面项 .ntd-help-menu-item，
  // 功能点子项 .ntd-help-menu-sub-item；点击页面项会选中并展开（helpTreeSelect 决策）。
  await page.locator('.ntd-help-menu-item', { hasText: '仪表盘' }).first().click();
  await page.waitForTimeout(400);
  const childTitle = page.locator('.ntd-help-menu-sub-item', { hasText: 'Tab 切换' });
  await expect(childTitle.first()).toBeVisible();

  // 验证点 3：点击功能点子节点，右侧内容切换为对应 md。
  // expect.poll 等待内容区标题切换为目标文档，替代固定 500ms 等待。
  await childTitle.first().click();
  await expect.poll(async () => (await readContentInfo(page)).text ?? '').toContain('Tab');
  const feature = await readContentInfo(page);
  console.log('子节点内容标题:', JSON.stringify(feature.headings));
  expect(feature.headings.length).toBeGreaterThan(0);

  // 全程不应出现 X-Markdown 输入类型错误
  const xmdErrors = consoleErrors.filter(e => e.includes('X-Markdown'));
  console.log('X-Markdown 错误数:', xmdErrors.length);
  expect(xmdErrors).toHaveLength(0);
});
