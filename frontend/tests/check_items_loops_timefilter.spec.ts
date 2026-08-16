// check_items_loops_timefilter.spec.ts — 需求 111 功能验证。
//
// 验证范围（对应 docs/testing/111-事项环路看板时间过滤-测试.md §3）：
//   1. 事项列表形态：默认「全部」，选 6h 只保留窗口内事项
//   2. 事项卡片形态：时间窗与列表共享，Tab 卡片同口径收窄
//   3. 环路列表形态：默认「全部」，选 7d 只保留窗口内环路
//   4. 环路看板形态：默认 24h，选「全部」后不过滤
//   5. 回归：任务页看板时间分段仍可用（行为不变）
//
// baseURL 由环境变量 NTD_TEST_BASE_URL 覆盖：独立 worktree 验证实例（18089）
// 与日常 dev 实例（18088）互不干扰，默认仍指向 18088。
import { test, expect } from '@playwright/test';

const BASE = process.env.NTD_TEST_BASE_URL || 'http://localhost:18088';

// 时间分段选中项的文本（antd Segmented 给选中项加 ant-segmented-item-selected）
const SELECTED_SEG = '.ant-segmented-item-selected';

// ── 种子数据探测 ──────────────────────────────────────────────
// 本 spec 验证时间窗过滤，依赖「窗口内 / 窗口外」两组对比数据（见测试文档 §3 的
// 预置方法）。为避免在他人 dev 实例或 CI 上因缺种子数据而误报失败：
// beforeAll 探测种子数据是否存在，缺失时整组用例自动跳过（不误报、不污染他人环境）。
let hasSeed = false;
test.beforeAll(async ({ request }) => {
  try {
    // 取第一个工作空间 id（workspace 端点无鉴权，dev 实例下必有默认空间）
    const wsRes = await request.get(BASE + '/api/v1/workspaces');
    const wsBody = await wsRes.json().catch(() => ({}));
    const wsId = (wsBody as { data?: Array<{ id?: number }> }).data?.[0]?.id;
    if (wsId == null) return;
    // 并行探测事项中心与环路列表的种子标题
    const [todosBody, loopsBody] = await Promise.all([
      request.get(BASE + '/api/v1/workspaces/' + wsId + '/todos/center').then(r => r.json().catch(() => ({}))),
      request.get(BASE + '/api/v1/workspaces/' + wsId + '/loops').then(r => r.json().catch(() => ({}))),
    ]);
    const todoTitles = (((todosBody as { data?: { items?: Array<{ title?: string }> } }).data?.items) ?? [])
      .map(i => i.title);
    const loopNames = (((loopsBody as { data?: Array<{ name?: string }> }).data) ?? [])
      .map(l => l.name);
    hasSeed =
      todoTitles.includes('窗口内事项') && todoTitles.includes('窗口外事项')
      && loopNames.includes('最近环路') && loopNames.includes('老旧环路');
  } catch {
    // 实例不可达（连接拒绝）时 request.get 直接 reject：视为无种子，
    // 由各用例的 test.skip 统一跳过，避免在无 dev 实例环境误报失败。
    hasSeed = false;
  }
});

test('事项列表：默认全部，选 6h 只剩窗口内事项', async ({ page }) => {
  test.skip(!hasSeed, '缺少时间过滤种子数据（见 spec 头部说明）');
  await page.goto(BASE + '/#/todos?view=list');
  // 默认「全部」：两条种子事项都可见
  await expect(page.getByText('窗口内事项')).toBeVisible();
  await expect(page.getByText('窗口外事项')).toBeVisible();
  // 选 6h → 窗口外事项（100h 前创建）消失
  await page.locator('.ant-segmented').getByText('6h').click();
  await expect(page.getByText('窗口内事项')).toBeVisible();
  await expect(page.getByText('窗口外事项')).toHaveCount(0);
  await page.screenshot({ path: 'tests/__screenshots__/111-todos-list-6h.png', fullPage: true });
  // 切回「全部」恢复
  await page.locator('.ant-segmented').getByText('全部').click();
  await expect(page.getByText('窗口外事项')).toBeVisible();
});

test('事项卡片：时间窗与列表共享，Tab 卡片同口径', async ({ page }) => {
  test.skip(!hasSeed, '缺少时间过滤种子数据（见 spec 头部说明）');
  await page.goto(BASE + '/#/todos?view=card');
  // 默认「全部」：手动触发 Tab 下两条都在
  await expect(page.getByText('窗口内事项')).toBeVisible();
  await expect(page.getByText('窗口外事项')).toBeVisible();
  // 选 24h → 窗口外（100h 前）消失
  await page.locator('.ant-segmented').getByText('24h').click();
  await expect(page.getByText('窗口内事项')).toBeVisible();
  await expect(page.getByText('窗口外事项')).toHaveCount(0);
  await page.screenshot({ path: 'tests/__screenshots__/111-todos-card-24h.png', fullPage: true });
});

test('环路列表：默认全部，选 7d 只剩窗口内环路', async ({ page }) => {
  test.skip(!hasSeed, '缺少时间过滤种子数据（见 spec 头部说明）');
  await page.goto(BASE + '/#/loops?view=list');
  await expect(page.getByText('最近环路')).toBeVisible();
  await expect(page.getByText('老旧环路')).toBeVisible();
  await page.locator('.ant-segmented').getByText('7d').click();
  await expect(page.getByText('最近环路')).toBeVisible();
  await expect(page.getByText('老旧环路')).toHaveCount(0);
  await page.screenshot({ path: 'tests/__screenshots__/111-loops-list-7d.png', fullPage: true });
});

test('环路看板：默认 24h，可切「全部」', async ({ page }) => {
  test.skip(!hasSeed, '缺少时间过滤种子数据（见 spec 头部说明）');
  await page.goto(BASE + '/#/loops?view=kanban');
  // 默认保持历史行为：选中 24h
  await expect(page.locator(SELECTED_SEG).getByText('24h')).toBeVisible();
  // 选「全部」→ 选中项切换（执行历史不再收窄）
  await page.locator('.ant-segmented').getByText('全部').click();
  await expect(page.locator(SELECTED_SEG).getByText('全部')).toBeVisible();
  await page.screenshot({ path: 'tests/__screenshots__/111-loops-kanban-all.png', fullPage: true });
});

test('回归：任务页看板时间分段仍可用', async ({ page }) => {
  test.skip(!hasSeed, '缺少时间过滤种子数据（见 spec 头部说明）');
  await page.goto(BASE + '/#/tasks?view=kanban');
  // 任务页看板分段（showAll 形态）存在且可切换
  await expect(page.locator('.ant-segmented').getByText('全部')).toBeVisible();
  await page.locator('.ant-segmented').getByText('24h').click();
  await expect(page.locator(SELECTED_SEG).getByText('24h')).toBeVisible();
  await page.locator('.ant-segmented').getByText('全部').click();
  await expect(page.locator(SELECTED_SEG).getByText('全部')).toBeVisible();
  await page.screenshot({ path: 'tests/__screenshots__/111-tasks-kanban-regression.png', fullPage: true });
});
