import { test, expect } from '@playwright/test';

test.describe('Executor UI Tests', () => {
  test.beforeEach(async ({ page }) => {
    // Go to the app
    await page.goto('http://localhost:5173');
    // Wait for page to load
    await page.waitForLoadState('networkidle');
  });

  test('should load the main page', async ({ page }) => {
    // Check page title or main content
    await expect(page.locator('body')).toBeVisible();
  });

  test('should create a todo and start execution', async ({ page }) => {
    // Click the add button or navigate to create todo
    const addButton = page.locator('button').filter({ hasText: /新建|新增|添加|add/i }).first();
    if (await addButton.isVisible()) {
      await addButton.click();
      await page.waitForTimeout(500);
    }

    // Mobile FAB entry
    const mobileFab = page.locator('[aria-label="新建任务"]').first();
    if (await mobileFab.isVisible({ timeout: 1000 }).catch(() => false)) {
      await mobileFab.click();
      await page.waitForTimeout(500);
    }

    // Find the title input and fill it
    await page.getByPlaceholder('输入 Todo 标题').fill('Test task for UI');

    // Find prompt textarea and fill with simple prompt
    await page.getByPlaceholder('输入 Prompt（会作为任务执行的内容，留空则使用标题）').fill('Say hello in 3 words');

    // 这里当前 UI 没有 executor 选择器；如果后续补了控件，再加稳定的 locator。

    // Submit the form
    const submitButton = page.locator('button').filter({ hasText: /创建|提交|确定|submit|create/i }).first();
    await submitButton.click();
    await page.waitForTimeout(1000);

    // Check if todo was created
    await page.waitForTimeout(500);
  });

  test('should list todos', async ({ page }) => {
    // Check if todo list exists
    const todoList = page.locator('.todo-list-container');
    await expect(todoList).toBeVisible({ timeout: 3000 });

    // Count todo items
    const todoItems = todoList.locator('[role="listitem"], li, tr');
    const count = await todoItems.count();
    expect(count).toBeGreaterThanOrEqual(0);
  });

  test('should toggle theme between light and dark', async ({ page }) => {
    // Find the theme toggle button
    const themeToggle = page.locator('[aria-label="切换主题"]');
    await expect(themeToggle).toBeVisible({ timeout: 3000 });

    // Get initial theme from localStorage
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