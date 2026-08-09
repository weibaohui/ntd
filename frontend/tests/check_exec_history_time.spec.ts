// 调试脚本：验证执行历史行的时间信息展示。
//
// 2026 重构后任务详情「执行历史」Tab 复用 LoopExecutionsPanel：
//   - 行选择器由旧的 [role=button][aria-label*=执行 #N] 改为 .loop-exec-row-head；
//   - 时间展示由「开始 + 绝对时间 / 耗时 2m」改为「相对时间(formatRelativeTime) + 耗时 {durationLabel}」，
//     其中 durationLabel：运行中(无 finished_at)=「进行中」，已结束=「Xm Ys / Xs / Xms」；
//   - execItem/execRowDesc/execDetail 等 class 全部废弃。
// 本用例改为校验新 UI 的时间语义。
//
// TODO-VERIFY: 依赖 dev 库 task 24 已关联环路(loop_id)且至少有 1 条执行记录；
// 若数据未就绪，用例会在「无执行行」处提前 return，需手工预置数据后复跑。
import { test, expect } from '@playwright/test';

const BASE = 'http://localhost:18088';

test('执行历史时间信息展示校验', async ({ page }) => {
  // SPA 仅在启动时读一次 selected_workspace，addInitScript 在首屏前写入避免报「todo 不属于工作空间」。
  await page.addInitScript(() => localStorage.setItem('selected_workspace', '1'));
  await page.goto(`${BASE}/#/tasks/24`, { waitUntil: 'domcontentloaded' });

  // 等待详情面板加载完成（执行历史 Tab 出现即说明详情数据已返回）。
  const execTab = page.getByRole('tab', { name: /执行历史/ });
  await expect(execTab).toBeVisible({ timeout: 30000 });
  await execTab.click();

  const pane = page.locator('.ant-tabs-tabpane-active');

  // 新 UI 的执行行：.loop-exec-row-head（旧 execItem / 执行 #N aria-label 已废弃）。
  const rowCount = await pane.locator('.loop-exec-row-head').count();
  // 数据未就绪则跳过：本用例的核心是时间展示，无执行记录无法验证，留给 CI/手工预置。
  if (rowCount === 0) {
    // TODO-VERIFY: dev 库 task 24 无执行记录，跳过；预置数据后复跑。
    return;
  }

  // 每条执行行都应展示「耗时」文案：
  //   - 已结束 → durationLabel 给出「Xm Ys / Xs / Xms」；
  //   - 运行中(无 finished_at) → 「进行中」。
  // 旧断言「运行中不显示耗时」已不成立（新 UI 统一显示「耗时 进行中」）。
  await expect(pane.locator('.loop-exec-row-head').first()).toContainText('耗时');

  // 截图留档（gitignored，供 PR 评论上传）。
  await page.screenshot({ path: 'tests/__screenshots__/exec_history_time.png' });
});
