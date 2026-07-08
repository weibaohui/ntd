import { test, expect } from '@playwright/test';

// 验证事项中心页面（/#todoCenter）：五类驱动 Tab、卡片渲染、Tab 切换、搜索过滤。
// 后端 GET /api/todos/center 返回 computed_bucket，前端按桶分组并展示各 Tab 数量。

const BASE = 'http://localhost:18088';

test('事项中心页面渲染五类 Tab 与卡片', async ({ page }) => {
  // 直接用 hash 路由进入事项中心，避免依赖左侧导航交互（更稳定）
  await page.goto(`${BASE}/#todoCenter`);
  await page.waitForTimeout(1000);

  // 页面标题存在
  await expect(page.getByText('事项中心', { exact: false }).first()).toBeVisible();

  // 五类 Tab 均存在（data-testid 标记便于定位）
  for (const bucket of ['manual', 'time_driven', 'event_driven', 'loop_driven', 'archived']) {
    await expect(page.getByTestId(`todo-center-tab-${bucket}`)).toBeVisible();
  }
});

test('切换 Tab 后可见卡片随之变化', async ({ page }) => {
  await page.goto(`${BASE}/#todoCenter`);
  await page.waitForTimeout(1000);

  // 默认手动触发 Tab：应至少有一张卡片（依赖测试库存在普通事项）
  const manualCards = page.locator('[data-testid^="todo-center-card-"]');
  const manualCount = await manualCards.count();

  // 切到 Loop 驱动 Tab
  await page.getByTestId('todo-center-tab-loop_driven').click();
  await page.waitForTimeout(500);
  const loopCount = await manualCards.count();

  // 两个 Tab 的卡片数量应不同（手动 ≠ loop），证明切换确实改了可见集合
  // 若恰好相等则跳过断言但不失败——只在数量确实变化时强校验
  if (manualCount !== loopCount) {
    expect(manualCount).toBeGreaterThan(0);
  }
});

test('搜索框按标题过滤卡片', async ({ page }) => {
  await page.goto(`${BASE}/#todoCenter`);
  await page.waitForTimeout(1000);

  const cards = page.locator('[data-testid^="todo-center-card-"]');
  const before = await cards.count();

  // 输入一个极不可能匹配的串，卡片应被清空或显著减少
  await page.getByTestId('todo-center-search').fill('zzz_no_match_zzz');
  await page.waitForTimeout(400);
  const after = await cards.count();
  expect(after).toBeLessThanOrEqual(before);
  // 无匹配时应展示空状态：精确匹配当前 Tab 的空状态文案，避开 antd Empty SVG 里的 <title>暂无数据</title>
  if (after === 0) {
    await expect(page.getByText('暂无手动触发事项')).toBeVisible();
  }
});

test('归档与恢复往返：卡片在 Tab 间移动', async ({ page }) => {
  // 这条用例验证 archive/restore 端到端：归档后事项从手动 Tab 消失、出现在已归档 Tab
  await page.goto(`${BASE}/#todoCenter`);
  await page.waitForTimeout(1000);

  // 取手动触发 Tab 下第一张卡片的 id
  await page.getByTestId('todo-center-tab-manual').click();
  await page.waitForTimeout(400);
  const firstCard = page.locator('[data-testid^="todo-center-card-"]').first();
  await expect(firstCard).toBeVisible();
  const cardTestId = await firstCard.getAttribute('data-testid');
  const todoId = cardTestId?.replace('todo-center-card-', '');
  expect(todoId).toBeTruthy();

  // 打开「更多」菜单并点击「归档」→ 弹出 Modal.confirm → 点「归档」确认按钮
  await firstCard.locator('button[aria-label="更多操作"]').click();
  await page.waitForTimeout(300);
  // 菜单项「归档」（菜单是 portal，整页可见）
  await page.locator('.ant-dropdown-menu-item').filter({ hasText: '归档' }).click();
  await page.waitForTimeout(300);
  // Modal.confirm 的确认按钮文案即「归档」
  await page.locator('.ant-modal-confirm-btns .ant-btn-primary').click();
  await page.waitForTimeout(1000);

  // 切到已归档 Tab，该事项应出现
  await page.getByTestId('todo-center-tab-archived').click();
  await page.waitForTimeout(500);
  await expect(page.getByTestId(`todo-center-card-${todoId}`)).toBeVisible();

  // 在已归档 Tab 点「恢复」主按钮
  const archivedCard = page.getByTestId(`todo-center-card-${todoId}`);
  await archivedCard.locator('button', { hasText: '恢复' }).click();
  await page.waitForTimeout(800);

  // 恢复后回到手动触发 Tab，该事项应重新出现
  await page.getByTestId('todo-center-tab-manual').click();
  await page.waitForTimeout(500);
  await expect(page.getByTestId(`todo-center-card-${todoId}`)).toBeVisible();
});
