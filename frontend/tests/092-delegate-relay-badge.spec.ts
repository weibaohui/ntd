// 092-任务委派执行 P2：详情页「管家调度中 N/10」徽标验证。
// 验证点（对应需求 092 验收标准 7）：委派 + 专家 + 自动接力任务，详情头部展示调度进度徽标。
//
// 策略说明：
// - 走真实「新建任务」提交流程（非纯 UI 联动），因为徽标依赖后端落库的委派字段——
//   委派任务一经创建即带 execution_mode=delegate + auto_continue=1 + continue_rounds=0，
//   详情头部据此渲染徽标「管家调度中 0/10」。提交会用当前选中工作空间建任务，
//   天然规避「种子任务所在工作空间与默认选中不一致」的可达性问题。
// - 徽标在创建瞬间（continue_rounds=0）即可见；专家首跑尚未完成，故断言 N=0。
// - afterAll 删除本次创建的任务：finalize 接力入口会据此跳过（任务不存在 → 不再推进），
//   避免专家结论里若含 @ 触发失控接力链路，污染开发库。

import { test, expect } from '@playwright/test';
import { execSync } from 'child_process';

const BASE = process.env.E2E_BASE_URL ?? 'http://localhost:18088';
const DEV_DB = process.env.HOME + '/.ntd/data.dev.db';
// 用带标记的唯一需求，便于失败定位与 afterAll 按前缀清理残留。
const MARKER = 'E2E接力徽标校验任务';

// 按标题前缀硬删任务（连带其讨论帖/载体 todo）。开发库自洽优先于精确归属。
function cleanupSeededTasks() {
  try {
    const ids: string = execSync(
      `sqlite3 "${DEV_DB}" "SELECT IFNULL(GROUP_CONCAT(id),'') FROM tasks WHERE title LIKE '%${MARKER}%';"`,
    ).toString().trim();
    if (!ids) return;
    execSync(`sqlite3 "${DEV_DB}" "DELETE FROM task_posts WHERE task_id IN (${ids});"`);
    execSync(`sqlite3 "${DEV_DB}" "DELETE FROM execution_records WHERE source_todo_id IN (SELECT id FROM todos WHERE title LIKE '%${MARKER}%');"`);
    execSync(`sqlite3 "${DEV_DB}" "DELETE FROM todos WHERE title LIKE '%${MARKER}%';"`);
    execSync(`sqlite3 "${DEV_DB}" "DELETE FROM tasks WHERE id IN (${ids});"`);
  } catch {
    // 清理失败不影响测试结论（仅开发库残留），静默吞掉。
  }
}

test.describe('092 委派自动接力徽标', () => {
  test.afterAll(() => cleanupSeededTasks());

  test('委派+专家+自动接力任务详情展示「管家调度中 N/10」', async ({ page }) => {
    await page.goto(`${BASE}/#/tasks`);
    await page.waitForSelector('.ant-table-row, .ant-empty', { timeout: 15000 });

    // 打开新建弹窗。
    await page.getByRole('button', { name: /新建/ }).first().click();
    const dialog = page.getByRole('dialog', { name: /新建任务/ });
    await expect(dialog).toBeVisible();

    // 切到委派（处理人类型默认「专家」，无需切换 Segmented）。
    await dialog.locator('.ant-radio-button-wrapper', { hasText: '委派' }).click();
    await expect(dialog.locator('[data-testid="create-task-assignee"]')).toBeVisible();

    // 填需求（带唯一标记，便于定位新建出的任务行 + afterAll 清理）。
    await dialog.locator('[data-testid="create-task-requirement"]').fill(MARKER);

    // 选第一个专家（专家列表来自 getAllExperts，开发库已内置 50+ 专家）。
    await dialog.locator('[data-testid="create-task-assignee"]').click();
    const firstExpert = page.locator('.ant-select-item-option').first();
    await firstExpert.waitFor({ state: 'visible' });
    await firstExpert.click();

    // 打开自动接力开关（仅专家可用；默认关闭，这里手动开启以触发徽标渲染条件）。
    const autoSwitch = dialog.locator('[data-testid="create-task-auto-continue"]');
    await expect(autoSwitch).not.toHaveClass(/ant-switch-checked/);
    await autoSwitch.click();
    await expect(autoSwitch).toHaveClass(/ant-switch-checked/);

    // 提交：点「开始执行」。委派创建会立即建任务（执行在后台 spawn），故弹窗很快关闭。
    await page.getByRole('button', { name: '开始执行' }).click();
    await expect(dialog).toHaveCount(0, { timeout: 15000 });

    // 列表刷新后，新建任务（按 id DESC）排在最前。定位带标记的行并点开详情。
    const row = page.locator('.ant-table-row', { hasText: MARKER }).first();
    await row.waitFor({ state: 'visible', timeout: 15000 });
    await row.click();

    // 详情头部出现后，断言管家调度徽标：文案「管家调度中 N/10」，N 为已完成的接力轮数。
    // 创建瞬间首跑尚未完成 → N=0；用正则容纳「首跑恰好完成一轮」的竞态，避免误判。
    const badge = page.locator('.ant-tag', { hasText: /管家调度中\s*\d+\s*\/\s*10/ });
    await expect(badge).toBeVisible({ timeout: 15000 });
    await expect(badge).toHaveText(/管家调度中\s*0\s*\/\s*10/);

    // 截图留档（test-results 由 Playwright 管理且已 gitignore，不入库）。
    await page.screenshot({ path: 'test-results/092-delegate-relay-badge.png' });
  });
});
