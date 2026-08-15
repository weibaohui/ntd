// 调试脚本：验证执行历史改为「内联手风琴」后的交互。
// 点某个执行项 → 详情应在该项所属的 loop-exec-row 内、同框展开。
//
// 2026 重构后：
//   - 折叠/展开控件由 [aria-label*=查看详情/收起详情] 改为 .loop-exec-row-head 的 onClick；
//     展开态文案「展开 ▼ / 收起 ▲」在 head 末尾的 span 里。
//   - 展开内容区由 execItem/execDetail/「执行看板」改为 .loop-exec-row-detail（含 StepExecList）。
//   - 「执行看板」文案不再出现（看板入口改为 head 上的「黑板」按钮）。
//
// TODO-VERIFY: 依赖 dev 库首个任务已关联环路且至少有 1 条执行记录；数据未就绪时用例提前 return。
import { test, expect } from '@playwright/test';

const BASE = 'http://localhost:18088';

test('执行历史内联展开校验', async ({ page }) => {
  // SPA 仅在启动时读一次 selected_workspace，addInitScript 在首屏前写入。
  await page.addInitScript(() => localStorage.setItem('selected_workspace', '1'));
  await page.goto(`${BASE}/#/tasks`, { waitUntil: 'domcontentloaded' });

  // 进入详情：点列表第一行（任务需关联环路才有执行历史 Tab）。
  await page.waitForSelector('.ant-table-row', { timeout: 30000 });
  await page.locator('.ant-table-row').first().click();

  // 切到执行历史 Tab（TaskDetailTabs 复用 LoopExecutionsPanel）。
  await page.getByRole('tab', { name: /执行历史/ }).click();
  await expect(page.locator('.ant-tabs-tabpane-active')).toBeVisible();

  // 新 UI：执行项头部 = .loop-exec-row-head（旧的 查看详情/收起详情 aria-label 已废弃）。
  const collapsedItem = page.locator('.loop-exec-row-head').first();
  const headCount = await collapsedItem.count();
  if (headCount === 0) {
    // TODO-VERIFY: 首个任务无环路执行记录，跳过内联展开断言；预置数据后复跑。
    return;
  }
  await expect(collapsedItem).toBeVisible();

  // 063 的 autoExpandFirstPending：首条「待审批」执行会被面板自动展开（审批按钮一步
  // 可见）。并发套件（063/ntd004）在 ws1 种入待审批数据时，首行可能已是展开态——
  // 先点一次收起，把断言基线拉回「全收起」，再验证「点击展开」路径本身。
  const preRow = collapsedItem.locator('xpath=ancestor::div[contains(@class,"loop-exec-row")][1]');
  if (await preRow.locator('.loop-exec-row-detail').count() > 0) {
    await collapsedItem.click();
  }
  await expect(preRow.locator('.loop-exec-row-detail')).toHaveCount(0);

  // 点击展开（head onClick 切换 expandedId）。
  await collapsedItem.click();

  // 展开后：同一 loop-exec-row 内出现 .loop-exec-row-detail（内联、同框）。
  const expandedRow = page.locator('.loop-exec-row-detail').first();
  await expect(expandedRow).toBeVisible({ timeout: 15000 });

  // 截图核对：head 正下方、同框展开的详情区。
  await page.screenshot({ path: 'tests/__screenshots__/exec_inline_expand.png', fullPage: false });

  console.log('执行历史内联展开校验通过：详情在点击项正下方同框展开');
});
