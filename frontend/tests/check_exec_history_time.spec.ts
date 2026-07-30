// 调试脚本：验证任务详情「执行历史」tab 每行展示基本时间信息。
// 覆盖三种形态：已结束（开始时间+耗时）、进行中（仅开始时间）、无需求文本（desc 行不渲染）。
// 依赖开发库中 task 24 的 3 条测试执行记录（id 1/2/3，验证前由 sqlite3 手工插入）。
import { test, expect } from '@playwright/test';

const BASE = 'http://localhost:18088';

test('执行历史时间信息展示校验', async ({ page }) => {
  await page.goto(`${BASE}/#/tasks/24`, { waitUntil: 'domcontentloaded' });

  // 等待详情面板加载完成（执行历史 Tab 出现即说明详情数据已返回）。
  const execTab = page.getByRole('tab', { name: /执行历史/ });
  await expect(execTab).toBeVisible({ timeout: 30000 });
  await execTab.click();

  const pane = page.locator('.ant-tabs-tabpane-active');

  // 进行中执行（started 08:30Z 无 finished_at）：
  // 只显示「开始」，不出现「耗时」（耗时无从谈起，由 running 状态 Tag 表达）。
  const runningRow = pane.locator('[role="button"][aria-label*="执行 #3"]');
  await expect(runningRow).toBeVisible();
  await expect(runningRow).toContainText('开始');
  await expect(runningRow).not.toContainText('耗时');

  // 成功执行（08:00Z→08:02:15Z = 135s → formatDurationSec 分钟粒度「2m」）：
  // 同时显示开始时间与耗时。
  const successRow = pane.locator('[role="button"][aria-label*="执行 #1"]');
  await expect(successRow).toContainText('开始');
  await expect(successRow).toContainText('耗时 2m');

  // 失败执行（7/29 10:00Z→11:05:30Z = 3930s → 「1h5m」）：
  // 验证跨小时耗时格式；该条 trigger_meta 无 requirement，卡片内不应有 desc 行。
  const failedRow = pane.locator('[role="button"][aria-label*="执行 #2"]');
  await expect(failedRow).toContainText('耗时 1h5m');
  const failedCard = failedRow.locator('xpath=ancestor::div[contains(@class, "execItem")][1]');
  await expect(failedCard.locator('[class*="execRowDesc"]')).toHaveCount(0);

  // 原始 UTC ISO 串（含 'T' 与 'Z'）不应再出现在任何行内——时间已被本地格式化。
  await expect(pane).not.toContainText('T08:00:00');

  // 截图留档（gitignored，供 PR 评论上传）。
  await page.screenshot({ path: 'tests/__screenshots__/exec_history_time.png' });
});
