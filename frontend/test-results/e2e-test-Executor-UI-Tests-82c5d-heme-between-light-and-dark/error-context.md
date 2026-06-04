# Instructions

- Following Playwright test failed.
- Explain why, be concise, respect Playwright best practices.
- Provide a snippet of code with the fix, if possible.

# Test info

- Name: e2e-test.spec.ts >> Executor UI Tests >> should toggle theme between light and dark
- Location: e2e-test.spec.ts:59:3

# Error details

```
Error: page.goto: net::ERR_CONNECTION_REFUSED at http://localhost:5173/
Call log:
  - navigating to "http://localhost:5173/", waiting until "load"

```

# Test source

```ts
  1  | import { test, expect } from '@playwright/test';
  2  | 
  3  | test.describe('Executor UI Tests', () => {
  4  |   test.beforeEach(async ({ page }) => {
  5  |     // Go to the app
> 6  |     await page.goto('http://localhost:5173');
     |                ^ Error: page.goto: net::ERR_CONNECTION_REFUSED at http://localhost:5173/
  7  |     // Wait for page to load
  8  |     await page.waitForLoadState('networkidle');
  9  |   });
  10 | 
  11 |   test('should load the main page', async ({ page }) => {
  12 |     // Check page title or main content
  13 |     await expect(page.locator('body')).toBeVisible();
  14 |   });
  15 | 
  16 |   test('should create a todo and start execution', async ({ page }) => {
  17 |     // Click the add button or navigate to create todo
  18 |     const addButton = page.locator('button').filter({ hasText: /新建|新增|添加|add/i }).first();
  19 |     if (await addButton.isVisible()) {
  20 |       await addButton.click();
  21 |       await page.waitForTimeout(500);
  22 |     }
  23 | 
  24 |     // Mobile FAB entry
  25 |     const mobileFab = page.locator('[aria-label="新建任务"]').first();
  26 |     if (await mobileFab.isVisible({ timeout: 1000 }).catch(() => false)) {
  27 |       await mobileFab.click();
  28 |       await page.waitForTimeout(500);
  29 |     }
  30 | 
  31 |     // Find the title input and fill it
  32 |     await page.getByPlaceholder('输入 Todo 标题').fill('Test task for UI');
  33 | 
  34 |     // Find prompt textarea and fill with simple prompt
  35 |     await page.getByPlaceholder('输入 Prompt（会作为任务执行的内容，留空则使用标题）').fill('Say hello in 3 words');
  36 | 
  37 |     // 这里当前 UI 没有 executor 选择器；如果后续补了控件，再加稳定的 locator。
  38 | 
  39 |     // Submit the form
  40 |     const submitButton = page.locator('button').filter({ hasText: /创建|提交|确定|submit|create/i }).first();
  41 |     await submitButton.click();
  42 |     await page.waitForTimeout(1000);
  43 | 
  44 |     // Check if todo was created
  45 |     await page.waitForTimeout(500);
  46 |   });
  47 | 
  48 |   test('should list todos', async ({ page }) => {
  49 |     // Check if todo list exists
  50 |     const todoList = page.locator('.todo-list-container');
  51 |     await expect(todoList).toBeVisible({ timeout: 3000 });
  52 | 
  53 |     // Count todo items
  54 |     const todoItems = todoList.locator('[role="listitem"], li, tr');
  55 |     const count = await todoItems.count();
  56 |     expect(count).toBeGreaterThanOrEqual(0);
  57 |   });
  58 | 
  59 |   test('should toggle theme between light and dark', async ({ page }) => {
  60 |     // Find the theme toggle button
  61 |     const themeToggle = page.locator('[aria-label="切换主题"]');
  62 |     await expect(themeToggle).toBeVisible({ timeout: 3000 });
  63 | 
  64 |     // Get initial theme from localStorage
  65 |     const initialTheme = await page.evaluate(() => localStorage.getItem('app_theme'));
  66 | 
  67 |     // Click to toggle theme
  68 |     await themeToggle.click();
  69 |     await page.waitForTimeout(500);
  70 | 
  71 |     // Verify theme changed
  72 |     const newTheme = await page.evaluate(() => localStorage.getItem('app_theme'));
  73 |     expect(newTheme).not.toBe(initialTheme);
  74 | 
  75 |     // Toggle back
  76 |     await themeToggle.click();
  77 |     await page.waitForTimeout(500);
  78 | 
  79 |     // Verify theme reverted
  80 |     const revertedTheme = await page.evaluate(() => localStorage.getItem('app_theme'));
  81 |     expect(revertedTheme).toBe(initialTheme);
  82 |   });
  83 | });
```