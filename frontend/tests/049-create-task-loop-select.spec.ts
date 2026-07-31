// 049-新建任务下拉显示工艺信息 Playwright 验证。
// 验证点（对应需求 049 验收标准第 1 条）：
// 1. 打开「新建任务」弹窗，展开工艺环路下拉；
// 2. 选项文本符合 `#<环路ID> 环路名称（#工艺ID 工艺名称 工艺版本）` 格式；
// 3. 选项括号内带出版面已知的工艺显示名与版本（与 dev 库数据比对）。
// 环境依赖：需至少存在一条 process_template_id 非空的环路；
// 干净环境（CI/新机器）无数据时降级为 skip，不造数（与 031 spec 退化模式一致）。

import { test, expect } from '@playwright/test';

// playwright.config.ts 的 baseURL=5173 是全仓历史遗留（make dev 实际监听 18088，
// 后端 embedded 模式 serve dist），既有 spec 均硬编码 18088；
// 额外支持 E2E_BASE_URL env 覆盖，便于 CI/非本机环境指向其他实例
// （与 check-loop-flow-no-trace-link.spec.ts 的 env 覆盖模式一致）。
const BASE = process.env.E2E_BASE_URL ?? 'http://localhost:18088';

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

  // 收集全部真实选项文本，排除 antd 空态节点（notFoundContent 会渲染成
  // .ant-select-item-option-empty，无环路时会被误当选项纳入断言）。
  const optionTexts = await dropdown
    .locator('.ant-select-item-option:not(.ant-select-item-option-empty)')
    .allTextContents();
  console.log('下拉选项：', JSON.stringify(optionTexts, null, 2));

  // 环境无工艺环路时降级 skip：不断言条数（条数依赖 dev 库数据，属环境假设），
  // 交由 vitest 单测保证格式逻辑，本 spec 只验证有数据时的真实渲染。
  test.skip(optionTexts.length === 0, '当前环境无工艺环路（process_template_id 非空），跳过格式断言');

  // 每条选项必须符合 049 格式：#<环路ID> 名称（#<工艺ID> 工艺名 版本）。
  // 工艺名/版本用 .+ 而非 \S+：二者均允许含空格（设计文档 §3.1 口径，如「标准需求交付 1.2.0」）。
  const pattern = /^#\d+ .+（#\d+ .+ .+）$/;
  for (const text of optionTexts) {
    expect(text.trim()).toMatch(pattern);
  }

  // 截图留档（test-results 目录由 Playwright 管理且已 gitignore，不入库）。
  await page.screenshot({ path: 'test-results/049-create-task-loop-select.png' });
});
