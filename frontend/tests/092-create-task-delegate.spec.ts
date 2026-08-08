// 092-任务委派执行：新建任务「执行方式」切换与委派 UI 联动验证。
// 验证点（对应需求 092 验收标准 1/2/5）：
// 1. 默认工艺环路：显示环路下拉，不显示处理人/接力字段；
// 2. 切到委派：显示处理人选择 + 自动接力开关，环路下拉消失；
// 3. 处理人选执行器 → 自动接力开关禁用；切回专家 → 恢复可用（仅专家可托管）。
// 纯前端 UI 联动，不真正提交（避免触发真实执行）。

import { test, expect } from '@playwright/test';

// playwright.config.ts 的 baseURL=5173 是历史遗留（make dev 实际监听 18088，
// 后端 embedded 模式 serve dist）；沿用既有 spec 的 env 覆盖 + 默认 18088 模式。
const BASE = process.env.E2E_BASE_URL ?? 'http://localhost:18088';

test('新建任务：工艺环路↔委派 切换与委派联动', async ({ page }) => {
  await page.goto(`${BASE}/#/tasks`);
  // 等任务列表首屏渲染（环路列表随列表接口一并拉取，用于 Modal 下拉）。
  await page.waitForSelector('.ant-table-row, .ant-empty', { timeout: 15000 });

  // 打开「新建任务」弹窗（data-testid 落在无尺寸的包装上，须用 dialog 角色节点断言）。
  await page.getByRole('button', { name: /新建/ }).first().click();
  const dialog = page.getByRole('dialog', { name: /新建任务/ });
  await expect(dialog).toBeVisible();

  // 1. 默认工艺环路：环路下拉可见，处理人/接力字段尚未渲染。
  await expect(dialog.locator('[data-testid="create-task-loop-select"]')).toBeVisible();
  await expect(dialog.locator('[data-testid="create-task-assignee"]')).toHaveCount(0);

  // 2. 切到委派：处理人选择 + 自动接力开关出现，环路下拉移除。
  await dialog.locator('.ant-radio-button-wrapper', { hasText: '委派' }).click();
  await expect(dialog.locator('[data-testid="create-task-assignee"]')).toBeVisible();
  await expect(dialog.locator('[data-testid="create-task-auto-continue"]')).toBeVisible();
  await expect(dialog.locator('[data-testid="create-task-loop-select"]')).toHaveCount(0);

  // 委派默认处理人类型为专家：自动接力开关应可用。
  const autoSwitch = dialog.locator('[data-testid="create-task-auto-continue"]');

  // 3. 切到执行器：自动接力开关必须禁用（执行器无调度能力）。
  await dialog.locator('.ant-segmented-item', { hasText: '执行器' }).click();
  await expect(autoSwitch).toHaveClass(/ant-switch-disabled/);

  // 切回专家：自动接力开关恢复可用。
  await dialog.locator('.ant-segmented-item', { hasText: '专家' }).click();
  await expect(autoSwitch).not.toHaveClass(/ant-switch-disabled/);

  // 截图留档（test-results 目录由 Playwright 管理且已 gitignore，不入库）。
  await page.screenshot({ path: 'test-results/092-create-task-delegate.png' });
});

// #8：专家列表加载失败必须给用户可见的错误提示，而非只 console.warn 让用户面对空下拉。
test('专家列表加载失败时弹出错误提示（#8）', async ({ page }) => {
  // 拦截 /api/v1/experts 返回 code!=0，触发 getAllExperts reject（模拟专家索引读取失败）。
  // 包成 {code,message} 信封，对齐前端 client 拦截器（code!=0 即 reject）的口径。
  await page.route('**/api/v1/experts', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({ code: 1, message: '专家索引读取失败' }),
    }),
  );
  await page.goto(`${BASE}/#/tasks`);
  await page.waitForSelector('.ant-table-row, .ant-empty', { timeout: 15000 });

  // 打开 Modal 即触发专家列表拉取；失败应弹 message.error，而非静默空下拉（#8）。
  await page.getByRole('button', { name: /新建/ }).first().click();
  await expect(page.locator('.ant-message-error')).toContainText('专家列表加载失败', { timeout: 8000 });
});

// #10：处理人类型专家↔执行器切换时，已选处理人必须清空，避免把专家名当执行器名提交。
test('切换处理人类型时清空已选处理人（#10）', async ({ page }) => {
  // 注入一个确定性专家，避免依赖真实后端专家数据导致用例不稳。
  await page.route('**/api/v1/experts', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        code: 0,
        data: [{ name: '测试专家', display_name_zh: '测试专家甲' }],
      }),
    }),
  );
  await page.goto(`${BASE}/#/tasks`);
  await page.waitForSelector('.ant-table-row, .ant-empty', { timeout: 15000 });

  // 打开 Modal 同时等专家列表接口返回：mock 虽即时，但必须确保 React 已拿到数据再开下拉，
  // 否则 options 异步到达会触发 antd Select 重渲染、把已开的浮层关掉，造成选项 not-visible。
  const expertsLoaded = page.waitForResponse(
    (r) => r.url().includes('/api/v1/experts'),
    { timeout: 10000 },
  );
  await page.getByRole('button', { name: /新建/ }).first().click();
  await expertsLoaded;
  const dialog = page.getByRole('dialog', { name: /新建任务/ });
  await expect(dialog).toBeVisible();

  // 切到委派（默认处理人类型=专家）。
  await dialog.locator('.ant-radio-button-wrapper', { hasText: '委派' }).click();
  const assignee = dialog.locator('[data-testid="create-task-assignee"]');
  await expect(assignee).toBeVisible();

  // 选一个专家：点 testid 根打开下拉，再点选项选中。
  // （本仓 antd 版本无 .ant-select-selector / .ant-select-selection-item；选中值直接体现在
  //   Select 根节点的文本，浮层选项在 body portal 下、不属于 Select 根，故断言根文本即可。）
  await assignee.click();
  await page
    .locator('.ant-select-dropdown:not(.ant-select-dropdown-hidden) .ant-select-item-option')
    .first()
    .click();
  // 选中后 Select 根文本应显示该专家名（而非 placeholder「选择一个专家」）。
  await expect(assignee).toContainText('测试专家甲');

  // 切到执行器：专家名与执行器名属不同命名空间，残留旧值会把专家名当执行器名提交（#10）。
  await dialog.locator('.ant-segmented-item', { hasText: '执行器' }).click();
  // onChange 清空 assigneeName → Select 回到 placeholder 态，不再残留专家名。
  await expect(assignee).not.toContainText('测试专家甲');
  await expect(assignee).toContainText('选择一个执行器');

  await page.screenshot({ path: 'test-results/092-assignee-clear-on-kind-switch.png' });
});
