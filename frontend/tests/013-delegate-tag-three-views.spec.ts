// NTD-013：验证委派任务（专家+自动接力）在三视图的委派信息 Tag 文案一致——
// 统一为「委派给：<处理人>（专家）🚀自动接力」，且状态旁「委派」徽标可见。
// 这是修复 Card/Kanban 从「委派：…🚀接力」漂移回统一口径的回归点。
//
// 策略：走真实新建委派任务（专家+自动接力），保证三视图各至少有一个目标 Tag 可断言。
// 仅校验列表渲染层文案，不等专家后台执行完成——委派 Tag 仅依赖 execution_mode=delegate，
// 与执行状态无关。afterAll 用 sqlite3 清理种子任务，防失控接力污染开发库。

import { test, expect, type Page } from '@playwright/test';
import { execSync } from 'child_process';

const BASE = process.env.E2E_BASE_URL ?? 'http://localhost:18088';
const DEV_DB = process.env.HOME + '/.ntd/data.dev.db';
// 用带标记的唯一标题，便于失败定位与 afterAll 按前缀清理残留。
const MARKER = 'E2E三视图委派Tag校验任务';
// 修复后三视图统一的委派 Tag 文案：同一 Tag 同时含「委派给：」前缀与「🚀自动接力」后缀。
// 修复前 Card/Kanban 为「委派：…🚀接力」，不命中此正则——正好作为回归断言。
const DELEGATE_TAG_RE = /委派给：.*🚀自动接力/;

// 按标题前缀硬删任务（连带讨论帖/载体 todo/执行记录）。开发库自洽优先于精确归属。
function cleanupSeededTasks() {
  try {
    const ids: string = execSync(
      `sqlite3 "${DEV_DB}" "SELECT IFNULL(GROUP_CONCAT(id),'') FROM tasks WHERE title LIKE '%${MARKER}%';"`,
    ).toString().trim();
    if (!ids) return;
    execSync(`sqlite3 "${DEV_DB}" "DELETE FROM task_posts WHERE task_id IN (${ids});"`);
    execSync(
      `sqlite3 "${DEV_DB}" "DELETE FROM execution_records WHERE source_todo_id IN (SELECT id FROM todos WHERE title LIKE '%${MARKER}%');"`,
    );
    execSync(`sqlite3 "${DEV_DB}" "DELETE FROM todos WHERE title LIKE '%${MARKER}%';"`);
    execSync(`sqlite3 "${DEV_DB}" "DELETE FROM tasks WHERE id IN (${ids});"`);
  } catch (e) {
    // 清理失败不影响测试结论（仅开发库残留），但记录原因便于排查
    // （如 sqlite3 未安装 / 库被占用 / 表已无残留），避免空 catch 吞错。
    console.warn('[013 cleanup] 清理种子任务失败（开发库残留，通常可忽略）：', e);
  }
}

// 新建一个「专家+自动接力」委派任务，触发三视图委派 Tag 渲染。
async function createExpertRelayDelegate(page: Page) {
  await page.goto(`${BASE}/#/tasks`);
  await page.waitForSelector('.ant-table-row, .ant-empty', { timeout: 15000 });
  await page.getByRole('button', { name: /新建/ }).first().click();
  const dialog = page.getByRole('dialog', { name: /新建任务/ });
  await expect(dialog).toBeVisible();
  // 切委派（默认处理人类型=专家，无需切 Segmented）。
  await dialog.locator('.ant-radio-button-wrapper', { hasText: '委派' }).click();
  await expect(dialog.locator('[data-testid="create-task-assignee"]')).toBeVisible();
  // 需求即任务标题来源（createTask 只下发 requirement），填唯一标记。
  await dialog.locator('[data-testid="create-task-requirement"]').fill(MARKER);
  // 选第一个专家（专家列表来自 getAllExperts，开发库内置 50+ 专家）。
  await dialog.locator('[data-testid="create-task-assignee"]').click();
  const firstExpert = page.locator('.ant-select-item-option').first();
  await firstExpert.waitFor({ state: 'visible' });
  await firstExpert.click();
  // 开自动接力：仅专家可用，开启后 Tag 末尾追加「🚀自动接力」——本用例核心验证点。
  const autoSwitch = dialog.locator('[data-testid="create-task-auto-continue"]');
  await autoSwitch.click();
  await expect(autoSwitch).toHaveClass(/ant-switch-checked/);
  // 提交：委派创建即建任务，后台 spawn 执行，弹窗很快关闭。
  await page.getByRole('button', { name: '开始执行' }).click();
  await expect(dialog).toHaveCount(0, { timeout: 15000 });
}

test.describe('013 委派任务三视图 Tag 文案一致', () => {
  test.afterAll(() => cleanupSeededTasks());

  test('Table/Kanban/Card 三视图均渲染「委派给：…🚀自动接力」+「委派」徽标', async ({ page }) => {
    await createExpertRelayDelegate(page);

    // 等新建任务出现在 Table（默认 list 视图，id DESC 排最前）。
    await page
      .locator('.ant-table-row', { hasText: MARKER })
      .first()
      .waitFor({ state: 'visible', timeout: 15000 });

    // --- Table 视图 ---
    const tableView = page.locator('[data-testid="tasks-table-view"]');
    await expect(tableView.locator('.ant-tag', { hasText: DELEGATE_TAG_RE }).first()).toBeVisible();
    await expect(tableView.getByText('委派', { exact: true }).first()).toBeVisible();

    // 视图切换 Segmented：list=0 / kanban=1 / card=2（按 options 顺序）。
    const toggle = page.locator('[data-testid="tasks-view-toggle"] .ant-segmented-item');

    // --- Kanban 视图 ---
    await toggle.nth(1).click();
    await page.waitForSelector('[data-testid="tasks-kanban-board"]', { timeout: 10000 });
    const kanban = page.locator('[data-testid="tasks-kanban-board"]');
    await expect(kanban.locator('.ant-tag', { hasText: DELEGATE_TAG_RE }).first()).toBeVisible();
    await expect(kanban.getByText('委派', { exact: true }).first()).toBeVisible();

    // --- Card 视图 ---
    await toggle.nth(2).click();
    await page.waitForSelector('[data-testid="tasks-card-view"]', { timeout: 10000 });
    const cardView = page.locator('[data-testid="tasks-card-view"]');
    await expect(cardView.locator('.ant-tag', { hasText: DELEGATE_TAG_RE }).first()).toBeVisible();
    await expect(cardView.getByText('委派', { exact: true }).first()).toBeVisible();

    // 截图留档（test-results 由 Playwright 管理且已 gitignore，不入库）。
    await page.screenshot({ path: 'test-results/013-delegate-tag-three-views.png' });
  });
});
