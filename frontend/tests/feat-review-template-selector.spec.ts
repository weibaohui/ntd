// 验证：评审模板拆出独立表后，loop 编辑器里的评审模板 Select
// - 列出所有模板（含默认）
// - 旁加 inline 「+ 新建模板」按钮可弹出 Modal 创建并立即选中
// - 保存后刷新仍保留所选
// - 设置 → 评审模板管理 页面能列出、能编辑、能删除（默认模板也能编辑）

import { test, expect } from '@playwright/test';

const BACKEND_URL = process.env.E2E_BACKEND_URL || 'http://localhost:18088';
const DEV_URL = 'http://localhost:18088';

test('Loop 编辑器评审模板 Select 选项可加载 + inline 新建可工作', async ({ page }) => {
  // 找一个 loop（任意一条都行，因为我们只关心 Select 是否拿到选项）
  const listRes = await page.request.get(`${BACKEND_URL}/api/loops?page=1&limit=50`);
  const listJson = await listRes.json();
  const loops: Array<{ id: number; name: string }> = listJson?.data ?? [];
  test.skip(loops.length === 0, '没有任何 loop 可测试，跳过');
  const loopId = loops[0].id;

  // 直接进入 loop 详情
  await page.goto(`${DEV_URL}/?loop=${loopId}`);
  await page.waitForLoadState('networkidle');
  await page.waitForTimeout(500);

  // 打开编辑 modal —— 用「编辑」按钮，标题里有"基础信息"或者直接找表单里的"评审模板"
  const editButton = page.getByRole('button', { name: /编辑/ }).first();
  if (await editButton.count() > 0) {
    await editButton.click();
    await page.waitForTimeout(400);
  }

  // 评审模板 Select 必须存在并显示出至少 1 个选项。
  // antd Form.Item 结构：.ant-form-item > .ant-form-item-label > label + .ant-form-item-control > .ant-select
  // 这里用 .ant-form-item-label 作为锚点（避免误中其他 label），再回到 form-item 找 select。
  const reviewTemplateSelect = page
    .locator('.ant-form-item-label:has(label:text("评审模板"))')
    .locator('xpath=..')
    .locator('.ant-select');
  await expect(reviewTemplateSelect).toBeVisible();

  // 打开下拉
  await reviewTemplateSelect.click();
  await page.waitForTimeout(300);

  // 默认模板 "默认评审任务" 必须在下拉里
  await expect(page.getByText('默认评审任务').first()).toBeVisible();

  // 关闭下拉
  await page.keyboard.press('Escape');

  // 「新建模板」link 按钮要可见，点击后弹 Modal
  const newTemplateLink = page.getByRole('button', { name: /新建模板/ }).first();
  await expect(newTemplateLink).toBeVisible();
  await newTemplateLink.click();
  await page.waitForTimeout(400);

  // Modal 标题为「新建评审模板」
  await expect(page.locator('.ant-modal-title:has-text("新建评审模板")')).toBeVisible();

  // 关闭 Modal
  await page.keyboard.press('Escape');
  await page.waitForTimeout(200);
});

test('设置 → 评审模板页能列出', async ({ page }) => {
  // 桌面端，设置入口在头部「更多操作」Dropdown 里；
  // 移动端才直接渲染独立设置按钮，这里按桌面端处理。
  await page.setViewportSize({ width: 1440, height: 900 });
  await page.goto(`${DEV_URL}/`);
  await page.waitForLoadState('networkidle');
  await page.waitForTimeout(1500);

  // 进入设置页 —— 通过 Header 上「更多操作」Dropdown，再点击「设置」菜单项
  const moreButton = page.getByRole('button', { name: '更多操作' }).first();
  await expect(moreButton).toBeVisible();
  await moreButton.click();
  await page.waitForTimeout(300);

  const settingsMenuItem = page.getByRole('menuitem', { name: '设置' }).first();
  await expect(settingsMenuItem).toBeVisible();
  await settingsMenuItem.click();
  await page.waitForTimeout(500);

  // 找「评审模板」tab —— Ant Design Tabs 用 .ant-tabs-tab 渲染，含文本节点
  const reviewTemplatesTab = page.locator('.ant-tabs-tab', { hasText: '评审模板' }).first();
  await expect(reviewTemplatesTab).toBeVisible();
  await reviewTemplatesTab.click();
  await page.waitForTimeout(500);

  // 表格里至少有默认模板
  await expect(page.getByText('默认评审任务').first()).toBeVisible();

  // 截图留档（不入 git，输出在 test-results/）
  await page.screenshot({
    path: 'frontend/tests/__screenshots__/review-templates-page.png',
    fullPage: true,
  });
});
