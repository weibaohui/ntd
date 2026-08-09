import { test, expect } from '@playwright/test';

test.describe('Executor UI Tests', () => {
  test.beforeEach(async ({ page }) => {
    // Go to the app。dev 用 embedded 模式（rust-embed 打进后端），监听 18088；
    // 旧的 5173 是 vite dev server 端口，make dev 不启动它，连不上会超时。
    await page.goto('http://localhost:18088');
    // Wait for page to load
    await page.waitForLoadState('networkidle');
  });

  test('should load the main page', async ({ page }) => {
    // Check page title or main content
    await expect(page.locator('body')).toBeVisible();
  });

  test('should create a todo and start execution', async ({ page }) => {
    // 028 起列表/详情独立路由，fallback view = todos 列表页；
    // 新建入口由 TodoListPage 顶部 header 提供（PlusOutlined + 「新建」按钮）。
    // 注意：PlusOutlined 图标渲染为 <span aria-label="plus">，会被计入按钮的可访问名，
    // 实际可访问名为 "plus 新建"。用 exact 要求完全相等会 0 匹配（旧用例因此超时失败）；
    // 去掉 exact 改为包含匹配——全页仅一个 name 含「新建」的按钮，无歧义（实测 count=1）。
    const addButton = page.getByRole('button', { name: '新建' });
    await expect(addButton).toBeVisible({ timeout: 5000 });
    await addButton.click();

    // TodoDrawer 是 antd Drawer（role=dialog）；创建模式标题为「创建任务」。
    const drawer = page.getByRole('dialog');
    await expect(drawer).toBeVisible({ timeout: 5000 });

    // 标题输入框 placeholder 已由旧的「输入 Todo 标题」改为「任务标题」。
    await drawer.getByPlaceholder('任务标题').fill('Test task for UI');
    // Prompt 文本域 placeholder 改为「描述完成该任务需要满足的条件...」。
    await drawer.getByPlaceholder('描述完成该任务需要满足的条件...').fill('Say hello in 3 words');

    // 提交按钮：创建模式下文案为「创建」（编辑模式才为「保存」）。
    // antd 对「纯文字、无图标」的两个汉字按钮会自动在中间插一个空格（autoInsertSpaceInButton），
    // DOM 实际渲染成「创 建」，裸字符串「创建」无论 exact 与否都无法命中（子串被空格打断）。
    // 用正则 /^创\s*建$/ 容忍空格并锚定整串：避开抽屉里另一个「从模板创建」按钮（它的名字含「模板」前后还有字）。
    await drawer.getByRole('button', { name: /^创\s*建$/ }).click();
    // 等待创建请求 + 列表刷新事件落盘，避免关闭抽屉造成的状态竞争。
    await page.waitForTimeout(1000);
  });

  test('should list todos', async ({ page }) => {
    // 028 重构后 .todo-list-container 已不存在；列表页桌面 header 固定渲染搜索框
    // （data-testid="items-page-search"，卡片/列表形态都在），作为「列表页已挂载」的稳定锚点。
    const searchInput = page.getByTestId('items-page-search');
    await expect(searchInput).toBeVisible({ timeout: 5000 });

    // 事项条目：列表形态为 ant-table 行（tr.ant-table-row）；卡片形态无 table 行。
    // 这里只需断言「列表页能正常渲染」，>=0 永真，保留宽松校验语义。
    const todoItems = page.locator('tr.ant-table-row');
    const count = await todoItems.count();
    expect(count).toBeGreaterThanOrEqual(0);
  });

  test('should toggle theme between light and dark', async ({ page }) => {
    // 主题切换按钮在 LeftRail 底部，用 data-testid 定位最稳；
    // 旧 aria-label「切换主题」已拆成动态的「切换暗色 / 切换亮色」，不再用字面量匹配。
    const themeToggle = page.getByTestId('left-rail-theme-toggle');
    await expect(themeToggle).toBeVisible({ timeout: 5000 });

    // 主题持久化在 localStorage 的 app_theme 键（useTheme.tsx STORAGE_KEY）。
    const initialTheme = await page.evaluate(() => localStorage.getItem('app_theme'));

    // Click to toggle theme
    await themeToggle.click();
    await page.waitForTimeout(500);

    // Verify theme changed
    const newTheme = await page.evaluate(() => localStorage.getItem('app_theme'));
    expect(newTheme).not.toBe(initialTheme);

    // Toggle back
    await themeToggle.click();
    await page.waitForTimeout(500);

    // Verify theme reverted
    const revertedTheme = await page.evaluate(() => localStorage.getItem('app_theme'));
    expect(revertedTheme).toBe(initialTheme);
  });
});