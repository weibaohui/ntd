import { test, expect } from '@playwright/test';

test.describe('Skill 文件浏览器功能', () => {
  test.beforeEach(async ({ page }) => {
    // 访问 Skills 页面
    await page.goto('http://localhost:18088');
    await page.waitForLoadState('networkidle');

    // 等待 Skills 面板加载
    await page.waitForSelector('text=Skills', { timeout: 10000 });
  });

  test('点击 Skill 卡片应显示详情抽屉', async ({ page }) => {
    // 等待 Skill 卡片加载
    await page.waitForSelector('.skill-card', { timeout: 10000 });

    // 点击第一个 Skill 卡片
    const firstCard = page.locator('.skill-card').first();
    await firstCard.click();

    // 等待抽屉打开
    await page.waitForSelector('.ant-drawer-content', { timeout: 5000 });

    // 验证抽屉标题包含 Skill 名称
    const drawerTitle = page.locator('.ant-drawer-title');
    await expect(drawerTitle).toBeVisible();
  });

  test('浏览文件按钮应能打开文件浏览器模态框', async ({ page }) => {
    // 等待 Skill 卡片加载
    await page.waitForSelector('.skill-card', { timeout: 10000 });

    // 点击第一个 Skill 卡片
    const firstCard = page.locator('.skill-card').first();
    await firstCard.click();

    // 等待抽屉打开
    await page.waitForSelector('.ant-drawer-content', { timeout: 5000 });

    // 点击"浏览文件"按钮
    await page.click('text=浏览文件');

    // 验证模态框打开
    await expect(page.locator('.ant-modal')).toBeVisible();

    // 验证文件浏览器内容显示
    await expect(page.locator('input[placeholder="搜索文件..."]')).toBeVisible();
  });

  test('文件浏览器应显示搜索框和文件统计', async ({ page }) => {
    // 等待 Skill 卡片加载
    await page.waitForSelector('.skill-card', { timeout: 10000 });

    // 点击第一个 Skill 卡片
    const firstCard = page.locator('.skill-card').first();
    await firstCard.click();

    // 等待抽屉打开
    await page.waitForSelector('.ant-drawer-content', { timeout: 5000 });

    // 点击"浏览文件"按钮
    await page.click('text=浏览文件');

    // 验证模态框打开
    await expect(page.locator('.ant-modal')).toBeVisible();

    // 验证搜索框存在
    await expect(page.locator('input[placeholder="搜索文件..."]')).toBeVisible();

    // 验证文件统计显示（共 X 个文件）
    await expect(page.locator('text=/共 \\d+ 个文件/')).toBeVisible();
  });

  test('搜索框应能过滤文件列表', async ({ page }) => {
    // 等待 Skill 卡片加载
    await page.waitForSelector('.skill-card', { timeout: 10000 });

    // 点击第一个 Skill 卡片
    const firstCard = page.locator('.skill-card').first();
    await firstCard.click();

    // 等待抽屉打开
    await page.waitForSelector('.ant-drawer-content', { timeout: 5000 });

    // 点击"浏览文件"按钮
    await page.click('text=浏览文件');

    // 验证模态框打开
    await expect(page.locator('.ant-modal')).toBeVisible();

    // 在搜索框中输入内容
    const searchInput = page.locator('input[placeholder="搜索文件..."]');
    await searchInput.fill('SKILL');

    // 等待过滤结果
    await expect(page.locator('text=/共 \\d+ 个文件/')).toBeVisible();

    // 清空搜索框
    await searchInput.clear();
  });

  test('点击文件应显示文件预览', async ({ page }) => {
    // 等待 Skill 卡片加载
    await page.waitForSelector('.skill-card', { timeout: 10000 });

    // 点击第一个 Skill 卡片
    const firstCard = page.locator('.skill-card').first();
    await firstCard.click();

    // 等待抽屉打开
    await page.waitForSelector('.ant-drawer-content', { timeout: 5000 });

    // 点击"浏览文件"按钮
    await page.click('text=浏览文件');

    // 验证模态框打开
    await expect(page.locator('.ant-modal')).toBeVisible();

    // 点击第一个文件（使用 data-testid 或更稳定的选择器）
    const firstFile = page.locator('[role="treeitem"]').first();
    await expect(firstFile).toBeVisible();
    await firstFile.click();

    // 验证文件预览区域显示（文件路径头部）
    await expect(page.locator('.ant-modal').locator('text=SKILL.md').or(page.locator('.ant-modal [role="treeitem"]'))).toBeVisible();
  });
});
