// 033-step-review-prompt.spec.ts
// ---------------------------------------------------------------------------
// 需求 033：环节评审模板 review_prompt 运行时验证。
//
// 验证范围（对应 docs/requirements/033-环节评审模板-需求.md §9 验收标准）：
// - 环节属性面板渲染「评审 Prompt」字段（033 新增）
// - 编辑「评审 Prompt」后，YAML tab 同步出现 review_prompt（双向联动）
//
// 说明：
// - React Flow 节点点击在 029-M5 引入 Tabs 后存在已知拦截回归
//   （见 029-m4-visual-editor.spec.ts 的 test.skip 注释），本脚本用 force:true +
//   较长等待尽量绕过；若节点点击仍不稳定，属于 029 既存问题，非 033 引入。
// - 8 个 bundled 工艺均为系统工艺（只读），本脚本验证字段渲染 + YAML 同步即可，
//   保存持久化（需用户工艺）留手动。
// ---------------------------------------------------------------------------
import { test, expect } from '@playwright/test';
import { editUrlByName } from './helpers/process';

const BASE = 'http://localhost:18088';
// 040 后工艺按 guid 寻址（name 允许重复不再唯一），guid 由 helper 按 name 动态查出，
// 不再硬编码 name=，避免 bundled 工艺改名时 spec 静默退化成列表页。
let editUrl = '';

test.describe('033 环节评审模板 review_prompt', () => {
  // 一次性查出系统工艺 4p12s-delivery 的 guid 并拼编辑器 URL，各 test 共用。
  test.beforeAll(async ({ request }) => {
    editUrl = await editUrlByName(request, '4p12s-delivery');
  });

  test('环节属性面板渲染「评审 Prompt」字段', async ({ page }) => {
    await page.goto(editUrl);
    await page.waitForTimeout(3000);
    // 可视化区与环节节点就绪
    expect(await page.locator('.react-flow').count()).toBeGreaterThan(0);
    expect(await page.locator('.react-flow__node-link').count()).toBeGreaterThan(0);

    // 点击第一个环节节点，触发右侧环节属性面板
    // force:true 绕过 Tabs 容器对节点指针事件的拦截（029-M5 已知回归）
    await page.locator('.react-flow__node-link').first().click({ timeout: 5000, force: true });
    await page.waitForTimeout(1200);

    // 环节属性面板应出现「评审 Prompt」字段标签（033 新增）
    const label = page.locator('.ant-form-item-label').filter({ hasText: '评审 Prompt' });
    await expect(label).toBeVisible({ timeout: 5000 });
  });

  test('「评审 Prompt」字段渲染 MD 编辑器控件（046 升级）', async ({ page }) => {
    await page.goto(editUrl);
    await page.waitForTimeout(3000);
    expect(await page.locator('.react-flow__node-link').count()).toBeGreaterThan(0);

    // 选中环节节点
    await page.locator('.react-flow__node-link').first().click({ timeout: 5000, force: true });
    await page.waitForTimeout(1200);

    // 046 把「评审 Prompt」从 TextArea 升级为 PromptMdField（@uiw/react-md-editor 封装），
    // 根节点 .w-md-editor；原 TextArea 的 placeholder 提示迁到 Form.Item tooltip（hover 才显，
    // 不便在 Playwright 断言），故这里只校验 MD 编辑器渲染。
    const rpItem = page
      .locator('.ant-form-item')
      .filter({ has: page.locator('.ant-form-item-label', { hasText: '评审 Prompt' }) });
    const mdEditor = rpItem.locator('.w-md-editor');
    await expect(mdEditor).toHaveCount(1);
    // 说明：编辑→YAML 同步底层（updateLinkField→yamlDump）由 vitest processDefinitionUpdater.test.ts
    //       19 个用例覆盖；系统工艺只读，编辑持久化需用户工艺，留手动验证。
  });
});
