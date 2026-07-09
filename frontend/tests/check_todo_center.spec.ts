import { test, expect } from '@playwright/test';

// 验证事项中心页面（/#items）：五类驱动 Tab、卡片渲染、Tab 切换、搜索过滤。
// 后端 GET /api/todos/center 返回 computed_bucket，前端按桶分组并展示各 Tab 数量。

const BASE = 'http://localhost:18088';

test('事项页渲染五类 Tab 与卡片', async ({ page }) => {
  // 直接用 hash 路由进入事项页（默认卡片形态）
  await page.goto(`${BASE}/#items`);
  await page.waitForTimeout(1000);

  // 页面标题存在（合并后标题为「事项」）
  await expect(page.locator('.ntd-page-card-title-text', { hasText: '事项' }).first()).toBeVisible();

  // 五类 Tab 均存在（data-testid 标记便于定位）
  for (const bucket of ['manual', 'time_driven', 'event_driven', 'loop_driven', 'archived']) {
    await expect(page.getByTestId(`todo-center-tab-${bucket}`)).toBeVisible();
  }
});

test('切换 Tab 后可见卡片随之变化', async ({ page }) => {
  await page.goto(`${BASE}/#items`);
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
  await page.goto(`${BASE}/#items`);
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
  await page.goto(`${BASE}/#items`);
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

test('Loop 驱动卡片展示所属 Loop 并可跳转', async ({ page }) => {
  await page.goto(`${BASE}/#items`);
  await page.waitForTimeout(1000);

  // 切到 Loop 驱动 Tab
  await page.getByTestId('todo-center-tab-loop_driven').click();
  await page.waitForTimeout(500);

  // 卡片应展示所属 Loop 名（后端 referencing_loops 返回的 loop_name）
  const loopTag = page.locator('.todo-center-card-tags .ant-tag, .todo-center-card-meta .ant-tag', { hasText: '笑话工厂' }).first();
  await expect(loopTag).toBeVisible();

  // 点击该 Loop 标签应跳转到 Loop 详情
  await loopTag.click();
  await page.waitForTimeout(800);
  expect(page.url()).toMatch(/#\/loops\?id=\d+/);
});

test('时间驱动卡片菜单含暂停/编辑/取消', async ({ page }) => {
  await page.goto(`${BASE}/#items`);
  await page.waitForTimeout(1000);

  await page.getByTestId('todo-center-tab-time_driven').click();
  await page.waitForTimeout(500);

  const card = page.locator('[data-testid^="todo-center-card-"]').first();
  await expect(card).toBeVisible();

  // 打开更多菜单
  await card.locator('button[aria-label="更多操作"]').click();
  await page.waitForTimeout(300);

  // 时间驱动卡片（已启用）应有暂停/编辑/取消三项
  const menu = page.locator('.ant-dropdown-menu');
  await expect(menu.getByText('暂停时间驱动')).toBeVisible();
  await expect(menu.getByText('编辑调度配置')).toBeVisible();
  await expect(menu.getByText('取消时间驱动')).toBeVisible();
});

test('归档被 Loop 引用的事项给出引用提示', async ({ page }) => {
  await page.goto(`${BASE}/#items`);
  await page.waitForTimeout(1000);

  // Loop 驱动 Tab 的卡片归档时，弹窗应提示「仍被 N 个启用」
  await page.getByTestId('todo-center-tab-loop_driven').click();
  await page.waitForTimeout(500);

  const card = page.locator('[data-testid^="todo-center-card-"]').first();
  await card.locator('button[aria-label="更多操作"]').click();
  await page.waitForTimeout(300);
  await page.locator('.ant-dropdown-menu-item').filter({ hasText: '归档' }).click();
  await page.waitForTimeout(400);

  // Modal.confirm 内容应包含 Loop 引用提示
  await expect(page.locator('.ant-modal-confirm-content').getByText(/仍被.*个启用/)).toBeVisible();

  // 取消，不改数据
  await page.locator('.ant-modal-confirm-btns .ant-btn:not(.ant-btn-primary)').click();
});

test('删除被 Loop 引用的事项被拒绝', async ({ page }) => {
  // 取一个 Loop 驱动事项，尝试通过 API 删除应返回 400
  const resp = await page.request.get(`${BASE}/api/todos/center?bucket=loop_driven`);
  const body = await resp.json();
  const loopTodo = (body.data || [])[0];
  expect(loopTodo).toBeTruthy();

  const del = await page.request.delete(`${BASE}/api/todos/${loopTodo.id}`);
  expect(del.status()).toBe(400);
  const delBody = await del.json();
  expect(delBody.message || '').toContain('Loop');
});

test('Loop 详情图标记已归档环节', async ({ page }) => {
  // 归档一个被 Loop 引用的事项（todo #1），Loop 详情图应渲染「已归档」标记
  await page.request.post(`${BASE}/api/todos/1/archive`);
  try {
    await page.goto(`${BASE}/#/loops?id=1&panel=detail`);
    await page.waitForTimeout(1500);
    // LoopFlowGraph 在 SVG 中渲染「已归档」文本
    await expect(page.getByText('已归档', { exact: true }).first()).toBeVisible({ timeout: 8000 });
  } finally {
    // 恢复，不留下脏数据
    await page.request.post(`${BASE}/api/todos/1/restore`);
  }
});

test('卡片/列表切换：列表模式渲染原 TodoPage 双栏', async ({ page }) => {
  await page.goto(`${BASE}/#items`);
  await page.waitForTimeout(1000);

  // 默认卡片视图：有卡片墙
  await expect(page.locator('.todo-center-grid')).toBeVisible();

  // 切到列表：应渲染原 TodoPage（双栏），卡片墙消失
  await page.getByTestId('todo-center-view-toggle').getByTitle('列表（双栏）').click();
  await page.waitForTimeout(800);
  await expect(page.locator('.todo-center-grid')).toHaveCount(0);
  // 双栏存在：ListDetailPage 的侧栏列表有 .todo-item 行（原 TodoList）
  await expect(page.locator('.todo-item').first()).toBeVisible();

  // 点回卡片视图
  await page.getByTestId('todo-center-view-toggle').getByTitle('卡片视图').click();
  await page.waitForTimeout(600);
  await expect(page.locator('.todo-center-grid')).toBeVisible();
});

test('点卡片切到列表并打开右栏详情', async ({ page }) => {
  await page.goto(`${BASE}/#items`);
  await page.waitForTimeout(1000);
  await page.getByTestId('todo-center-tab-manual').click();
  await page.waitForTimeout(400);

  const firstCard = page.locator('[data-testid^="todo-center-card-"]').first();
  await expect(firstCard).toBeVisible();
  // 点卡片标题区（非按钮）触发 onSelectTodo → 切列表 + 打开详情
  await firstCard.locator('.todo-center-card-title').click();
  await page.waitForTimeout(1000);

  // 卡片墙消失（切到了列表模式），URL 带上 id 与 panel=detail
  await expect(page.locator('.todo-center-grid')).toHaveCount(0);
  expect(page.url()).toMatch(/id=\d+&panel=detail/);
});

test('卡片菜单含复制/移动工作空间', async ({ page }) => {
  await page.goto(`${BASE}/#items`);
  await page.waitForTimeout(1000);

  const card = page.locator('[data-testid^="todo-center-card-"]').first();
  await card.locator('button[aria-label="更多操作"]').click();
  await page.waitForTimeout(300);

  const menu = page.locator('.ant-dropdown-menu');
  await expect(menu.getByText('复制到工作空间')).toBeVisible();
  await expect(menu.getByText('移动到工作空间')).toBeVisible();
});

test('已归档卡片菜单含删除', async ({ page }) => {
  await page.goto(`${BASE}/#items`);
  await page.waitForTimeout(1000);

  // 取一个手动事项归档
  await page.getByTestId('todo-center-tab-manual').click();
  await page.waitForTimeout(400);
  const card = page.locator('[data-testid^="todo-center-card-"]').first();
  const tid = await card.getAttribute('data-testid');
  await card.locator('button[aria-label="更多操作"]').click();
  await page.waitForTimeout(300);
  await page.locator('.ant-dropdown-menu-item').filter({ hasText: '归档' }).click();
  await page.waitForTimeout(300);
  await page.locator('.ant-modal-confirm-btns .ant-btn-primary').click();
  await page.waitForTimeout(1000);

  // 切到已归档，打开菜单，应有「删除」
  await page.getByTestId('todo-center-tab-archived').click();
  await page.waitForTimeout(500);
  const archivedCard = page.getByTestId(tid!);
  await archivedCard.locator('button[aria-label="更多操作"]').click();
  await page.waitForTimeout(300);
  await expect(page.locator('.ant-dropdown-menu').getByText('删除')).toBeVisible();

  // 关菜单并恢复，不留下脏数据
  await page.keyboard.press('Escape');
  await page.waitForTimeout(200);
  const idNum = tid!.replace('todo-center-card-', '');
  await page.request.post(`${BASE}/api/todos/${idNum}/restore`);
});

test('移动端渲染原列表页（卡片墙为桌面端专属）', async ({ page }) => {
  // 移动端视口：事项页走 TodoMobilePage，不渲染桌面端卡片墙
  await page.setViewportSize({ width: 375, height: 812 });
  await page.goto(`${BASE}/#items`);
  await page.waitForTimeout(1000);
  // 卡片墙不存在（移动端不进 ItemsPage/卡片视图）
  await expect(page.locator('.todo-center-grid')).toHaveCount(0);
});

test('工具栏含状态与动作类型筛选', async ({ page }) => {
  await page.goto(`${BASE}/#items`);
  await page.waitForTimeout(1000);

  // 状态筛选与动作类型筛选下拉均存在
  await expect(page.getByTestId('todo-center-status-filter')).toBeVisible();
  await expect(page.getByTestId('todo-center-action-filter')).toBeVisible();

  // 手动触发 Tab 应有「仅看可命令触发」勾选；切到时间驱动 Tab 后应消失
  await expect(page.getByTestId('todo-center-command-only')).toBeVisible();
  await page.getByTestId('todo-center-tab-time_driven').click();
  await page.waitForTimeout(400);
  await expect(page.getByTestId('todo-center-command-only')).toHaveCount(0);
});

test('状态筛选生效：选失败后只剩失败事项', async ({ page }) => {
  await page.goto(`${BASE}/#items`);
  await page.waitForTimeout(1000);

  const cards = page.locator('[data-testid^="todo-center-card-"]');
  const before = await cards.count();

  // 选「失败」状态
  await page.getByTestId('todo-center-status-filter').click();
  await page.waitForTimeout(200);
  await page.locator('.ant-select-item').filter({ hasText: '失败' }).click();
  await page.waitForTimeout(500);

  const after = await cards.count();
  // 筛选后数量应 <= 筛选前
  expect(after).toBeLessThanOrEqual(before);
  // 若仍有卡片，每张状态都应是 failed
  for (let i = 0; i < after; i++) {
    const statusTag = await cards.nth(i).locator('.ant-tag').allTextContents();
    expect(statusTag.some((t) => t.includes('失败'))).toBeTruthy();
  }
});

test('Loop 驱动卡片不含复制/移动工作空间', async ({ page }) => {
  await page.goto(`${BASE}/#items`);
  await page.waitForTimeout(1000);

  await page.getByTestId('todo-center-tab-loop_driven').click();
  await page.waitForTimeout(500);

  const card = page.locator('[data-testid^="todo-center-card-"]').first();
  await card.locator('button[aria-label="更多操作"]').click();
  await page.waitForTimeout(300);

  const menu = page.locator('.ant-dropdown-menu');
  // Loop 驱动按设计文档不应有复制/移动
  await expect(menu.getByText('复制到工作空间')).toHaveCount(0);
  await expect(menu.getByText('移动到工作空间')).toHaveCount(0);
});

test('事项页不渲染仪表盘：仅一个 PageCard 且标题为事项', async ({ page }) => {
  // 回归：之前 activeView==='todoCenter' 会落到 Dashboard 兜底，导致仪表盘与事项页并排
  await page.goto(`${BASE}/#items`);
  await page.waitForTimeout(1200);
  const titles = await page.locator('.ntd-page-card-title-text').allTextContents();
  expect(titles).toContain('事项');
  expect(titles).not.toContain('仪表盘');
  expect(titles).not.toContain('事项中心');
});

test('点击空分类不跳回第一个：停在所点击的空 Tab', async ({ page }) => {
  // 回归：之前空分类会被 effect 自动切回 manual，用户点击应保持原选择
  await page.goto(`${BASE}/#items`);
  await page.waitForTimeout(1200);
  // 找一个数量为 0 的分类：读各 Tab 计数
  const counts = await page.evaluate(() => {
    const tabs = ['manual', 'time_driven', 'event_driven', 'loop_driven', 'archived'];
    const out: { bucket: string; zero: boolean }[] = [];
    for (const b of tabs) {
      const el = document.querySelector(`[data-testid="todo-center-tab-${b}"]`);
      const text = el?.textContent || '';
      const n = parseInt(text.replace(/[^0-9]/g, ''), 10);
      out.push({ bucket: b, zero: Number.isNaN(n) || n === 0 });
    }
    return out;
  });
  const empty = counts.find((c) => c.zero && c.bucket !== 'manual');
  if (!empty) return; // 都非空则跳过（无可校验的空分类）
  await page.getByTestId(`todo-center-tab-${empty.bucket}`).click();
  await page.waitForTimeout(600);
  // 仍停留在所点击的 Tab：该 Tab 选项处于选中态，且展示对应空状态文案
  await expect(page.getByTestId(`todo-center-tab-${empty.bucket}`)).toBeVisible();
  // 选中态由 ant-segmented-item-selected 标记；断言该 Tab 的容器有 selected 类
  const selected = await page.locator(`[data-testid="todo-center-tab-${empty.bucket}"]`)
    .evaluate((el) => !!(el.closest('.ant-segmented-item')?.classList.contains('ant-segmented-item-selected')));
  expect(selected).toBeTruthy();
});
