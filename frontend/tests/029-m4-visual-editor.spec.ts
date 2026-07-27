// 029-m4-visual-editor.spec.ts
// ---------------------------------------------------------------------------
// M4 里程碑：React Flow 可视化编辑器运行时验证。
//
// 验证范围（对应 docs/design/029-M4-ReactFlow可视化编辑器-方案.md §5 验收标准）：
// - AC-M4-1：进入编辑器后，可视化区显示 React Flow 泳道编辑器
// - AC-M4-3：点击 link 节点，右侧属性面板显示环节属性表单
// - AC-M4-4：点击 phase 节点，右侧属性面板显示阶段属性表单
// - AC-M4-10：React Flow 画布支持 Controls / MiniMap
// - AC-M4-12：未选中节点时属性面板显示全局面板
//
// 注意：
// - 8 个 bundled 工艺都是系统工艺（is_system=true），M4 的系统工艺防护
//   会把 Monaco readOnly，但可视化区 + 属性面板仍应渲染（只读态）。
// - 路由用 query params 格式：?processMode=edit&name=xxx（非 path 风格）。
// - React Flow v12 节点选择器用 class：.react-flow__node-phase / .react-flow__node-link
//   （非 data-type 属性，v12 不输出 data-type）。
// ---------------------------------------------------------------------------

import { test, expect } from '@playwright/test';

const BASE = 'http://localhost:18088';
// 路由：query params 格式
const EDIT_URL = `${BASE}/#/processes?processMode=edit&name=4p12s-delivery`;

test.describe('029 M4 React Flow 可视化编辑器', () => {
  test('AC-M4-1: 进入编辑器后渲染可视化区与 React Flow 节点', async ({ page }) => {
    await page.goto(EDIT_URL);
    await page.waitForTimeout(3000);

    // 顶部 Alert 应存在（系统工艺黄色告警）
    const alert = page.locator('.ant-alert').first();
    await expect(alert).toBeVisible({ timeout: 8000 });

    // React Flow 容器应渲染
    const reactFlow = page.locator('.react-flow').first();
    await expect(reactFlow).toBeVisible({ timeout: 8000 });

    // 应至少渲染一个 phase 节点（泳道容器，class 选择器）
    const phaseNodes = page.locator('.react-flow__node-phase');
    const phaseCount = await phaseNodes.count();
    expect(phaseCount).toBeGreaterThan(0);
    console.log(`M4 渲染了 ${phaseCount} 个 phase 节点`);

    // 应至少渲染一个 link 节点（环节卡片，class 选择器）
    const linkNodes = page.locator('.react-flow__node-link');
    const linkCount = await linkNodes.count();
    expect(linkCount).toBeGreaterThan(0);
    console.log(`M4 渲染了 ${linkCount} 个 link 节点`);
  });

  test('AC-M4-10: React Flow 画布渲染 Controls 与 MiniMap', async ({ page }) => {
    await page.goto(EDIT_URL);
    await page.waitForTimeout(3000);

    await expect(page.locator('.react-flow').first()).toBeVisible({ timeout: 8000 });

    // Controls（缩放控制按钮组）应存在
    const controls = page.locator('.react-flow__controls');
    await expect(controls).toBeVisible({ timeout: 5000 });

    // MiniMap（小地图）应存在
    const minimap = page.locator('.react-flow__minimap');
    await expect(minimap).toBeVisible({ timeout: 5000 });
  });

  test('AC-M4-3: 点击 link 节点右侧属性面板切换到环节属性', async ({ page }) => {
    await page.goto(EDIT_URL);
    await page.waitForTimeout(3000);

    await expect(page.locator('.react-flow').first()).toBeVisible({ timeout: 8000 });

    // 点击第一个 link 节点
    const firstLink = page.locator('.react-flow__node-link').first();
    await firstLink.click({ timeout: 5000 });
    await page.waitForTimeout(800);

    // 右侧属性面板应出现环节属性表单
    // LinkPropertyForm 头部「环节属性」
    const linkFormTitle = page.locator('text=环节属性');
    await expect(linkFormTitle).toBeVisible({ timeout: 5000 });

    // 应出现 on_success / on_gate_fail 字段标签
    const onSuccessLabel = page.locator('.ant-form-item-label').filter({ hasText: 'on_success' });
    await expect(onSuccessLabel).toBeVisible({ timeout: 5000 });
  });

  test('AC-M4-4: 点击 phase 节点右侧属性面板切换到阶段属性', async ({ page }) => {
    await page.goto(EDIT_URL);
    await page.waitForTimeout(3000);

    await expect(page.locator('.react-flow').first()).toBeVisible({ timeout: 8000 });

    // 点击第一个 phase 节点头部文字（force:true 避免子节点拦截指针事件）
    // phase 头部含「▸ {name}」文字，定位到该文字区域
    const phaseHeaderText = page.locator('.react-flow__node-phase').first().locator('span').first();
    await phaseHeaderText.click({ timeout: 5000, force: true });
    await page.waitForTimeout(800);

    // 右侧属性面板应出现阶段属性表单
    // PhasePropertyForm 头部「阶段属性」
    const phaseFormTitle = page.locator('text=阶段属性');
    await expect(phaseFormTitle).toBeVisible({ timeout: 5000 });
  });

  test('AC-M4-12: 未选中节点时属性面板显示全局面板', async ({ page }) => {
    await page.goto(EDIT_URL);
    await page.waitForTimeout(3000);

    await expect(page.locator('.react-flow').first()).toBeVisible({ timeout: 8000 });

    // 点击画布空白处取消选中
    await page.locator('.react-flow__pane').first().click({ timeout: 5000 });
    await page.waitForTimeout(800);

    // 应出现全局面板标识：工艺元信息折叠面板
    const metaPanel = page.locator('text=工艺元信息');
    await expect(metaPanel).toBeVisible({ timeout: 5000 });
  });
});
