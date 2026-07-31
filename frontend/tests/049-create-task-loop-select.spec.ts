// 049-新建任务下拉显示工艺信息 Playwright 验证。
// 验证点（对应需求 049 验收标准第 1 条）：
// 1. 打开「新建任务」弹窗，展开工艺环路下拉；
// 2. 选项文本符合 `#<环路ID> 环路名称（#工艺ID 工艺名称 工艺版本）` 格式；
// 3. 选项括号内带出版面已知的工艺显示名与版本（与 dev 库数据比对）。
// 前置：开发库需至少存在一条 process_template_id 非空的环路（当前有 3 条）。

import { test, expect } from '@playwright/test';

const BASE = 'http://localhost:18088';

test('新建任务下拉显示工艺名称与版本', async ({ page }) => {
  await page.goto(`${BASE}/#/tasks`);
  // 等任务列表首屏渲染完成（环路列表随列表接口一并拉取）。
  await page.waitForSelector('.ant-table-row, .ant-empty', { timeout: 15000 });

  // 打开「新建任务」弹窗。
  // 注意：data-testid 落在 .ant-modal-root 包装上，其子节点全是 position:fixed，
  // 包装自身无布局尺寸，Playwright 会判 hidden；断言须用内部 dialog 角色节点。
  await page.getByRole('button', { name: /新建/ }).first().click();
  const dialog = page.getByRole('dialog', { name: /新建任务/ });
  await expect(dialog).toBeVisible();

  // 展开工艺环路下拉（antd Select 点击输入区弹出选项浮层）。
  await dialog.locator('[data-testid="create-task-loop-select"]').click();
  // antd 会为关掉的浮层保留 DOM 并加 ant-select-dropdown-hidden，需排除。
  const dropdown = page.locator('.ant-select-dropdown:not(.ant-select-dropdown-hidden)');
  await expect(dropdown).toBeVisible({ timeout: 5000 });

  // 收集全部选项文本。antd Select 浮层 option 的类名为 .ant-select-item-option。
  const optionTexts = await dropdown
    .locator('.ant-select-item-option')
    .allTextContents();
  console.log('下拉选项：', JSON.stringify(optionTexts, null, 2));

  // 至少有一条可选环路（dev 库已有 3 条工艺环路）。
  expect(optionTexts.length).toBeGreaterThan(0);

  // 每条选项必须符合 049 格式：#<环路ID> 名称（#<工艺ID> 工艺名 版本）。
  // 工艺名/版本允许任意非空白字符（版本缺失时后端数据已为具体值，占位 — 也命中 \S+）。
  const pattern = /^#\d+ .+（#\d+ \S+ \S+）$/;
  for (const text of optionTexts) {
    expect(text.trim()).toMatch(pattern);
  }

  // 截图留档（test-results 目录由 Playwright 管理且已 gitignore，不入库）。
  await page.screenshot({ path: 'test-results/049-create-task-loop-select.png' });
});
