// 033-panel-collapse-title.spec.ts
// ---------------------------------------------------------------------------
// 需求 033 相关 UI 验证（对应本仓库当前分支的两处改动）：
// 1) 环节属性面板的「评审 Prompt」参数条不再包含 {{max_output_chars}} 占位符芯片。
// 2) 右侧属性面板收起（收缩栏）后，窄条上显示当前面板标题（环节属性/阶段属性/工艺属性）
//    与向右箭头，让用户收起后仍可识别面板内容。
//
// 导航与节点点击沿用 033-step-review-prompt.spec.ts 的 force:true 方式，
// 绕过 M5 Tabs 容器对 React Flow 节点指针事件的已知拦截。
// ---------------------------------------------------------------------------
import { test, expect } from '@playwright/test';

const BASE = process.env.UI_BASE || 'http://localhost:18088';
const EDIT_URL = `${BASE}/#/processes?processMode=edit&name=4p12s-delivery`;

test.describe('033 面板改动验证', () => {
  test('评审 Prompt 参数条不含 max_output_chars', async ({ page }) => {
    await page.goto(EDIT_URL);
    await page.waitForTimeout(3000);
    expect(await page.locator('.react-flow__node-link').count()).toBeGreaterThan(0);

    // 选中第一个环节节点，打开环节属性面板
    await page.locator('.react-flow__node-link').first().click({ timeout: 5000, force: true });
    await page.waitForTimeout(1200);

    // 定位「评审 Prompt」字段所在 Form.Item
    const rpItem = page
      .locator('.ant-form-item')
      .filter({ has: page.locator('.ant-form-item-label', { hasText: '评审 Prompt' }) });
    await expect(rpItem).toBeVisible({ timeout: 5000 });

    // 参数条内不应再出现 max_output_chars 芯片（033 改动：移除无用占位符）
    const maxChip = rpItem.locator('code', { hasText: 'max_output_chars' });
    await expect(maxChip).toHaveCount(0);

    // 仍应保留 original_prompt / original_output / acceptance_criteria 三个有用芯片
    await expect(rpItem.locator('code', { hasText: 'original_prompt' })).toHaveCount(1);
    await expect(rpItem.locator('code', { hasText: 'acceptance_criteria' })).toHaveCount(1);
  });

  test('属性面板收起后窄条显示标题与向右箭头', async ({ page }) => {
    await page.goto(EDIT_URL);
    await page.waitForTimeout(3000);
    expect(await page.locator('.react-flow__node-link').count()).toBeGreaterThan(0);

    // 选中环节节点，使展开态标题为「环节属性」
    await page.locator('.react-flow__node-link').first().click({ timeout: 5000, force: true });
    await page.waitForTimeout(1200);

    // 展开态下应有「收起属性面板」按钮（工具栏向右箭头）
    const collapseBtn = page.locator('button[aria-label="收起属性面板"]');
    await expect(collapseBtn).toBeVisible({ timeout: 5000 });

    // 点击收起
    await collapseBtn.click();
    await page.waitForTimeout(800);

    // 收起态窄条：展开按钮可见，且包含竖排标题「环节属性」
    const expandBtn = page.locator('button[aria-label="展开属性面板"]');
    await expect(expandBtn).toBeVisible({ timeout: 5000 });
    await expect(expandBtn).toContainText('环节属性');

    // 窄条内应含向右箭头（RightOutlined 渲染为 .anticon 的 svg）
    await expect(expandBtn.locator('svg').first()).toBeVisible();
  });
});
