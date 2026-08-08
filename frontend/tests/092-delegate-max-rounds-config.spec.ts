// 092-任务委派执行·护栏配置化：接力上限「任务级覆盖」端到端验证。
// 验证点（对应需求 092 护栏配置化）：
//  1. 新建任务 Modal：专家 + 自动接力开启时弹出「最大轮数」InputNumber；关闭开关则字段消失（preserve=false）。
//  2. 详情页徽标：管家调度徽标 N/M 的 M 来自三级解析有效值；点 ✎ 内联改上限 → M 随之变化；「恢复默认」回退。
//
// 策略说明：
// - 用例 1 纯前端 UI 联动，不提交（不触发真实执行）。
// - 用例 2 走真实「新建任务」提交（复用 badge spec 的建任务流程），因为徽标依赖后端落库的委派字段。
//   创建后到详情页点徽标弹 Popover，改上限并断言 M 变化——验证 PATCH /tasks/{id} 与详情重拉链路。
// - afterAll 删除本次创建的任务，避免专家结论若含 @ 触发失控接力污染开发库。

import { test, expect } from '@playwright/test';
import { execFileSync } from 'child_process';

const BASE = process.env.E2E_BASE_URL ?? 'http://localhost:18088';
const DEV_DB = process.env.HOME + '/.ntd/data.dev.db';
const MARKER = 'E2E护栏配置化任务';

// 按标题前缀硬删任务（连带讨论帖/载体 todo），开发库自洽优先于精确归属。
// 用 execFileSync 传 [db, sql] 参数数组而非 execSync 拼 shell 字符串：DEV_DB 取自环境变量，
// 经 shell 拼接存在命令注入面（OpenGrep SAST 报错）；execFileSync 不经 shell，参数原样传给 sqlite3。
function cleanupSeededTasks() {
  try {
    // 先查出待删任务 id 列表（GROUP_CONCAT 拼成 "1,2,3"）；无则空串直接返回。
    const ids: string = execFileSync('sqlite3', [
      DEV_DB,
      `SELECT IFNULL(GROUP_CONCAT(id),'') FROM tasks WHERE title LIKE '%${MARKER}%';`,
    ]).toString().trim();
    if (!ids) return;
    // 按外键依赖顺序删除：task_posts（引用 tasks）→ execution_records（引用载体 todo）→ todos → tasks。
    execFileSync('sqlite3', [DEV_DB, `DELETE FROM task_posts WHERE task_id IN (${ids});`]);
    execFileSync('sqlite3', [
      DEV_DB,
      `DELETE FROM execution_records WHERE source_todo_id IN (SELECT id FROM todos WHERE title LIKE '%${MARKER}%');`,
    ]);
    execFileSync('sqlite3', [DEV_DB, `DELETE FROM todos WHERE title LIKE '%${MARKER}%';`]);
    execFileSync('sqlite3', [DEV_DB, `DELETE FROM tasks WHERE id IN (${ids});`]);
  } catch (e) {
    console.warn('[092-config cleanup] 清理种子任务失败（开发库残留，通常可忽略）：', e);
  }
}

test.describe('092 接力上限配置化', () => {
  test.afterAll(() => cleanupSeededTasks());

  // 用例 1：新建 Modal「最大轮数」字段随专家+自动接力联动（纯 UI，不提交）。
  test('新建任务：专家+自动接力时出现「最大轮数」字段，关闭则消失', async ({ page }) => {
    await page.goto(`${BASE}/#/tasks`);
    await page.waitForSelector('.ant-table-row, .ant-empty', { timeout: 15000 });

    await page.getByRole('button', { name: /新建/ }).first().click();
    const dialog = page.getByRole('dialog', { name: /新建任务/ });
    await expect(dialog).toBeVisible();

    // 切到委派（处理人类型默认「专家」）。
    await dialog.locator('.ant-radio-button-wrapper', { hasText: '委派' }).click();

    // 自动接力默认关闭：「最大轮数」字段尚未渲染。
    const autoSwitch = dialog.locator('[data-testid="create-task-auto-continue"]');
    await expect(dialog.locator('[data-testid="create-task-max-rounds"]')).toHaveCount(0);

    // 开启自动接力：「最大轮数」InputNumber 出现，placeholder 含「默认 N 轮」（工作空间有效默认）。
    // 注：本版本 antd 把 data-testid 直接落在 <input> 上，故直接断言该元素，不再 .locator('input')。
    await autoSwitch.click();
    const maxRounds = dialog.locator('[data-testid="create-task-max-rounds"]');
    await expect(maxRounds).toBeVisible();
    await expect(maxRounds).toHaveAttribute('placeholder', /默认\s*\d+\s*轮/);

    // 关闭自动接力：字段卸载即清值（preserve=false），不残留上限被误提交。
    await autoSwitch.click();
    await expect(dialog.locator('[data-testid="create-task-max-rounds"]')).toHaveCount(0);
  });

  // 用例 2：详情徽标内联改上限（走真实建任务 + PATCH）。
  test('详情徽标内联改上限：置值后 M 变化，恢复默认回退', async ({ page }) => {
    await page.goto(`${BASE}/#/tasks`);
    await page.waitForSelector('.ant-table-row, .ant-empty', { timeout: 15000 });

    // 建一个委派+专家+自动接力任务（复用 badge spec 流程）。
    await page.getByRole('button', { name: /新建/ }).first().click();
    const dialog = page.getByRole('dialog', { name: /新建任务/ });
    await expect(dialog).toBeVisible();
    await dialog.locator('.ant-radio-button-wrapper', { hasText: '委派' }).click();
    await dialog.locator('[data-testid="create-task-requirement"]').fill(MARKER);
    await dialog.locator('[data-testid="create-task-assignee"]').click();
    const firstExpert = page.locator('.ant-select-item-option').first();
    await firstExpert.waitFor({ state: 'visible' });
    await firstExpert.click();
    await dialog.locator('[data-testid="create-task-auto-continue"]').click();
    await page.getByRole('button', { name: '开始执行' }).click();
    await expect(dialog).toHaveCount(0, { timeout: 15000 });

    // 打开新建任务详情。
    const row = page.locator('.ant-table-row', { hasText: MARKER }).first();
    await row.waitFor({ state: 'visible', timeout: 15000 });
    await row.click();

    // 徽标可见（N/M，M 为有效上限）。
    const badge = page.locator('[data-testid="relay-badge"]');
    await expect(badge).toBeVisible({ timeout: 15000 });

    // 点徽标弹 Popover，输入 5 并确定 → M 应变为 5。
    // data-testid 直接落在 InputNumber 的 <input> 上，故直接定位该元素（不套 .locator('input')）。
    await badge.click();
    const maxInput = page.locator('[data-testid="relay-max-input"]');
    await expect(maxInput).toBeVisible({ timeout: 5000 });
    await maxInput.fill('5');
    await page.locator('[data-testid="relay-max-confirm"]').click();
    // PATCH + 详情重拉后，徽标 M=5（N 可能因后台执行变化，故只断言 /5）。
    await expect(badge).toHaveText(/\/\s*5\b/, { timeout: 10000 });

    // 再次点开，「恢复默认」→ M 回退到工作空间默认（= hint 显示的 fallback 值）。
    await badge.click();
    await expect(page.locator('[data-testid="relay-max-reset"]')).toBeVisible({ timeout: 5000 });
    // Spec#2：覆盖已置 5（badge M=5），但编辑器「留空=用工作空间默认（X 轮）」提示须显示 fallback
    // （清除覆盖后的真实回退值），不能误显成即将被清除的覆盖值 5——校验 fallback 与 effective 分离。
    const hintText = await page.locator('.ant-popover').getByText(/工作空间默认/).innerText();
    const fallbackNum = hintText.match(/工作空间默认（(\d+) 轮/)?.[1];
    expect(fallbackNum, '应能从 hint 解析出回退轮数').toMatch(/^\d+$/);
    await page.locator('[data-testid="relay-max-reset"]').click();
    // 恢复后 M 精确等于工作空间默认（fallbackNum）。原 not/5/ 断言在工作空间默认恰为 5 时会误失败，
    // 且「不是 5」也无法证明回退到了正确值；改为精确匹配，无论默认值是几都对。
    await expect(badge).toHaveText(new RegExp(`\\/\\s*${fallbackNum}\\b`), { timeout: 10000 });

    await page.screenshot({ path: 'test-results/092-delegate-max-rounds-config.png' });
  });
});
