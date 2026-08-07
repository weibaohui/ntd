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
