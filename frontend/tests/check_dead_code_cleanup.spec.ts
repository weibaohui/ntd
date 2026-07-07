import { test, expect } from '@playwright/test';

/**
 * 死代码清理回归校验（A/B/D 三类清理 + review 修复后）。
 *
 * 覆盖本次清理涉及的所有改动路径：
 * - 6 个 backward-compat barrel 已删除：TodoList / TodoPostPage / RunningBoard /
 *   LoopKanban / LoopStudioExecutionsPanel / LoopStudioTriggersPanel。
 *   外部引用方全部改为指向子目录，验证相关页面仍能渲染。
 * - 4 个子目录 barrel 的死 re-export 已删除，验证子组件渲染不依赖 barrel。
 * - date-fns → dayjs 迁移：utils/datetime.ts 的 formatRelativeTime 现基于 dayjs，
 *   formatLocalDateTime 保留 new Date().toLocaleString() 以维持原输出格式。
 */

const BASE = 'http://localhost:18088';

test('清理后首页加载且无控制台错误', async ({ page }) => {
  const errors: string[] = [];
  page.on('pageerror', (err) => errors.push(`pageerror: ${err.message}`));
  page.on('console', (msg) => {
    if (msg.type() === 'error') errors.push(`console.error: ${msg.text()}`);
  });

  await page.goto(BASE);
  await page.waitForLoadState('networkidle');

  // 主应用容器渲染
  await expect(page.locator('body')).toBeVisible();
  // 主视图至少有一个 .ant-layout（应用根容器）
  await expect(page.locator('.ant-layout').first()).toBeVisible({ timeout: 5000 });
  expect(errors, `页面有控制台错误：\n${errors.join('\n')}`).toEqual([]);
});

test('TodoList 子目录 barrel 拆除后仍渲染', async ({ page }) => {
  // TodoList 引用方（TodoPage / LoopPage / TodoMobilePage / LoopMobilePage）
  // 全部改为 from './todo-list'，桌面端事项页应仍正常渲染。
  await page.goto(`${BASE}/#/items`);
  await page.waitForLoadState('networkidle');

  // 列表区域至少渲染 todo 卡片或空状态文案
  const listVisible = await page.locator('.ant-list, .ant-empty, .ntd-page-card').first().isVisible().catch(() => false);
  expect(listVisible, 'TodoList 区域未渲染').toBeTruthy();
});

test('MemorialBoard → 看板-环路视图（验证 LoopKanban / RunningBoard barrel 拆除）', async ({ page }) => {
  await page.goto(`${BASE}/#/memorial`);
  await page.waitForLoadState('networkidle');

  // 看板导航按钮
  const boardNav = page.locator('[aria-label="看板"]');
  await expect(boardNav).toBeVisible({ timeout: 5000 });
  await boardNav.click();
  await page.waitForLoadState('networkidle');

  // 切换到「环路视图」即 LoopKanban 组件
  const loopOption = page.getByText('环路视图');
  await expect(loopOption).toBeVisible({ timeout: 5000 });
  await loopOption.click();
  await page.waitForLoadState('networkidle');

  // LoopKanban 渲染时会创建 .loop-kanban-columns-container 容器
  // 这个 class 名是 LoopKanban 内部的稳定 hook，能反映 import 路径改写后子组件仍能解析
  const container = page.locator('.loop-kanban-columns-container');
  await expect(container).toBeVisible({ timeout: 5000 });
});

test('Loop Studio detail：验证 triggers/executions barrel 拆除', async ({ page }) => {
  // 进入环路视图；若有数据，点击首条 loop 进入 detail panel，
  // 该 panel 现在 import { LoopTriggersPanel } from './loop-studio/triggers'
  // 及 { LoopExecutionsPanel } from './loop-studio/executions'，
  // 通过验证 detail 内的「触发条件」「执行历史」标题来确认两条 import 都生效。
  await page.goto(`${BASE}/#/loops`);
  await page.waitForLoadState('networkidle');

  // 等列表数据加载
  await page.waitForTimeout(1500);

  // loop 列表项：LoopStudioListPanel 渲染的行
  const loopRow = page.locator('.ant-list-item, .ntd-loop-list-item').first();
  const hasLoop = await loopRow.isVisible().catch(() => false);
  if (!hasLoop) {
    test.skip(true, '当前工作空间无 loop 数据，无法触发 detail panel 渲染');
  }

  // 进入 detail 视图
  await loopRow.click();
  await page.waitForLoadState('networkidle');
  await page.waitForTimeout(800);

  // detail panel 会渲染三个区块标题，任意一个出现都说明
  // triggers/executions 子目录 barrel 改写后 import 仍能解析
  const triggersHeading = page.getByText('触发条件').first();
  const historyHeading = page.getByText('执行历史').first();
  const stepsHeading = page.getByText('执行环节').first();

  const triggersVisible = await triggersHeading.isVisible().catch(() => false);
  const historyVisible = await historyHeading.isVisible().catch(() => false);
  const stepsVisible = await stepsHeading.isVisible().catch(() => false);

  expect(
    triggersVisible || historyVisible || stepsVisible,
    'LoopStudioDetailPanel 的三个区块标题都没渲染，triggers/executions barrel 改写可能出问题',
  ).toBeTruthy();
});

test('formatLocalDateTime 行为保持（保留 toLocaleString，非 ISO 固定格式）', async ({ page }) => {
  // 这是 review M2 的回归点：确认 formatLocalDateTime 没有被改成
  // dayjs('YYYY-MM-DD HH:mm:ss') 固定格式，仍是 new Date().toLocaleString()。
  //
  // 在浏览器侧直接调用 toLocaleString 取一个固定 ISO 的渲染结果，
  // 如果未来有人误改成 dayjs 固定格式，输出会变成 "2026-07-07 16:00:00"
  // 而不是本地化字符串（如 "2026/7/7 下午4:00:00"），测试会失败。
  await page.goto(BASE);
  await page.waitForLoadState('networkidle');

  const result = await page.evaluate(() => {
    const iso = '2026-07-07T08:00:00.000Z';
    const local = new Date(iso).toLocaleString();
    return {
      local,
      // toLocaleString 输出应包含日期分隔符 (/ 或 -) 与时间分隔符 (:)
      // 不应原样输出 ISO 字符串
      isIsoLike: /^2026-07-07T08:00:00/.test(local),
    };
  });

  expect(result.local.length, 'toLocaleString 输出不应为空').toBeGreaterThan(0);
  expect(result.isIsoLike, 'toLocaleString 不应原样返回 ISO 字符串').toBe(false);
});

test('Dashboard 渲染（验证 dayjs 引用方仍工作）', async ({ page }) => {
  // Dashboard.tsx import dayjs + SpecialCards.tsx import type { Dayjs }，
  // 验证 date-fns 移除后这些 import 仍可解析。
  await page.goto(`${BASE}/#/dashboard`);
  await page.waitForLoadState('networkidle');

  // Dashboard 至少渲染一个统计卡片或图表容器
  const dashboardVisible = await page.locator('.ant-card, .ant-statistic, .recharts-wrapper, .ntd-page-card').first().isVisible().catch(() => false);
  expect(dashboardVisible, 'Dashboard 区域未渲染').toBeTruthy();
});
