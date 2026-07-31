/**
 * 046 工艺编辑器提示词 MD 编辑控件 — UI 端到端验证。
 *
 * 测试范围：
 * 1. 环节属性面板：「提示词」「评审 Prompt」渲染为 MD 编辑器（.w-md-editor），原 antd TextArea 消失，
 *    且评审参数条与「使用默认值」按钮保留；
 * 2. 「评审 Prompt」参数点击在光标处插入（需求 046 关键行为：由尾部追加升级为光标插入）；
 * 3. 全局面板：「异常处理 Prompt」渲染为 MD 编辑器，参数条保留。
 *
 * 数据准备：直接复用开发库既有工艺 complex-refactor（bundled 模板，含多环节），无需种子数据。
 *
 * 关键约定：
 * - 工艺编辑页路由为 hash 形式 #/processes?processMode=edit&guid=<guid>（App.tsx useViewState）；
 * - React Flow 环节节点 data-id 以 link- 开头（processGraphBuilder 的节点 id 约定 link-${i}-${j}）；
 * - 画布配置 fitView，所有节点初始落在可视区内，可直接点击；
 * - 默认无选中节点 → 属性面板渲染全局表单（ProcessPropertyPanel 路由规则）。
 */

import { test, expect } from '@playwright/test';

// 开发服务地址（make dev 默认端口）
const BASE = 'http://localhost:18088';
// 开发库既有 bundled 工艺模板，多环节结构稳定，适合做编辑器验证
const PROCESS_GUID = '8b986558-c5a4-4477-a0da-8a5b8f444194';

test.beforeEach(async ({ page }) => {
  await page.goto(`${BASE}/#/processes?processMode=edit&guid=${PROCESS_GUID}`);
  // 环节节点出现 = 工艺定义已加载 + 可视化画布已渲染，此时属性面板也已就绪
  await page.waitForSelector('.react-flow__node[data-id^="link-"]', { timeout: 15000 });
});

/**
 * 点击第一个「完整落在视口内」的环节节点。
 * 超宽工艺（如 9 阶段的 complex-refactor）在 fitView 最小缩放限制下左右会超出视口，
 * 直接点 first() 会命中视口外节点导致 Playwright 点击超时，因此先页内筛选可视节点。
 */
async function clickVisibleLinkNode(page: import('@playwright/test').Page): Promise<void> {
  const visibleLinkId = await page.evaluate(() => {
    for (const el of document.querySelectorAll('.react-flow__node[data-id^="link-"]')) {
      const r = el.getBoundingClientRect();
      if (r.left >= 0 && r.top >= 0 && r.right <= window.innerWidth && r.bottom <= window.innerHeight) {
        return el.getAttribute('data-id');
      }
    }
    return null;
  });
  // 画布已 fitView 渲染环节节点，视口内必然存在至少一个可点击节点
  if (!visibleLinkId) throw new Error('视口内未找到可点击的环节节点');
  await page.locator(`.react-flow__node[data-id="${visibleLinkId}"]`).click();
}

test('环节属性面板的提示词与评审 Prompt 均为 MD 编辑器', async ({ page }) => {
  // 点击视口内的环节节点 → 右侧属性面板切换为环节属性表单
  await clickVisibleLinkNode(page);

  // 「提示词」Form.Item：必须是 MD 编辑器，且不再有 antd TextArea
  const promptItem = page.locator('.ant-form-item').filter({
    has: page.locator('label[title="提示词"]'),
  });
  await expect(promptItem.locator('.w-md-editor')).toHaveCount(1);
  await expect(promptItem.locator('textarea.ant-input')).toHaveCount(0);

  // 「评审 Prompt」Form.Item：必须是 MD 编辑器，原 TextArea 消失
  const reviewItem = page.locator('.ant-form-item').filter({
    has: page.locator('label[title="评审 Prompt"]'),
  });
  await expect(reviewItem.locator('.w-md-editor')).toHaveCount(1);
  await expect(reviewItem.locator('textarea.ant-input')).toHaveCount(0);

  // 参数条与「使用默认值」按钮必须保留（改造只换控件，不削减既有能力）
  await expect(reviewItem.getByText('{{original_output}}')).toBeVisible();
  await expect(reviewItem.getByText('使用默认值')).toBeVisible();
});

test('评审 Prompt 参数点击在光标处插入', async ({ page }) => {
  await clickVisibleLinkNode(page);

  const reviewItem = page.locator('.ant-form-item').filter({
    has: page.locator('label[title="评审 Prompt"]'),
  });
  // @uiw/react-md-editor 内部是真实 textarea，可直接 fill 驱动受控值
  const editorTextarea = reviewItem.locator('.w-md-editor textarea');
  await editorTextarea.waitFor();

  // 填入基准文本并等受控值回流，确保后续切片基于最新 value
  await editorTextarea.fill('abcdef');
  await expect(editorTextarea).toHaveValue('abcdef');

  // 光标放到 index 3（abc|def 之间），模拟用户在文本中间点击参数
  await editorTextarea.evaluate((el) => {
    const t = el as HTMLTextAreaElement;
    t.focus();
    t.selectionStart = 3;
    t.selectionEnd = 3;
  });
  await reviewItem.getByText('{{original_output}}').click();

  // 光标处插入的期望结果：abc{{original_output}}def（尾部追加则会是 abcdef{{original_output}}）
  await expect(editorTextarea).toHaveValue('abc{{original_output}}def');
});

test('全局面板的异常处理 Prompt 为 MD 编辑器', async ({ page }) => {
  // 默认无选中节点 → 属性面板即为全局表单，无需额外操作
  const abnormalItem = page.locator('.ant-form-item').filter({
    has: page.locator('label[title="异常处理 Prompt"]'),
  });
  await abnormalItem.waitFor();
  await expect(abnormalItem.locator('.w-md-editor')).toHaveCount(1);
  await expect(abnormalItem.locator('textarea.ant-input')).toHaveCount(0);

  // 异常处理参数条必须保留
  await expect(abnormalItem.getByText('{{abnormal_status}}')).toBeVisible();
});
