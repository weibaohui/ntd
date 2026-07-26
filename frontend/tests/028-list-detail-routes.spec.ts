// 028-列表详情独立路由：验证 todos/loops 命名空间的 URL 路由行为。
//
// 设计文档：docs/design/028-列表详情独立路由-设计.md §9.1
//
// 覆盖范围：
// 1. 列表页 URL 直接进入（/#/todos、/#/loops）
// 2. 列表/详情视图模式切换（Segmented）
// 3. 点击行/卡片跳转到详情独立路由（/#/todos/:id、/#/loops/:id）
// 4. 详情页刷新后 URL 保持
// 5. 旧 /#/items URL 不再被处理（不做兼容重定向，但应用不应崩溃）
//
// 注：依赖后端运行在 18088 端口，且至少有一条 todo / loop 数据用于详情跳转测试。
// 若数据不存在，详情跳转相关用例会跳过（不失败）。

import { test, expect } from '@playwright/test';

const BASE = 'http://localhost:18088';

/** 等待 hash 路由生效 + 页面渲染稳定的统一延迟。 */
const ROUTE_SETTLE_MS = 800;

test.describe('028 列表详情独立路由', () => {
  test.beforeEach(async ({ page }) => {
    // 进入应用首页，等待 LeftRail 出现表明应用已挂载
    await page.goto(`${BASE}/`);
    await page.waitForLoadState('domcontentloaded');
  });

  test('事项列表页：URL /#/todos 直接进入可渲染', async ({ page }) => {
    // 测试目的：验证直接以 hash 路由 /#/todos 进入时，应用能正确挂载列表页且 URL 不被改写
    // 数据边界：不依赖任何 todo 数据，仅校验页面标题容器渲染（无数据也应渲染空态）
    // 直接以 hash 路由进入事项列表
    await page.goto(`${BASE}/#/todos`);
    await page.waitForTimeout(ROUTE_SETTLE_MS);

    // URL 仍是 todos 列表（未被改写）
    await expect(page).toHaveURL(/\/#\/todos$/);

    // 页面标题应为「事项」（PageCard 标题）
    await expect(page.locator('.ntd-page-card-title-text', { hasText: '事项' }).first()).toBeVisible();
  });

  test('事项列表页：视图模式切换 Segmented 存在', async ({ page }) => {
    // 测试目的：验证 028 新增的「卡片/列表」视图模式切换控件在列表页正确挂载
    // 数据边界：不依赖 todo 数据，仅校验 testid 元素存在
    await page.goto(`${BASE}/#/todos`);
    await page.waitForTimeout(ROUTE_SETTLE_MS);

    // 视图模式 Segmented 控件存在（卡片/列表切换入口）
    const toggle = page.getByTestId('todo-center-view-toggle');
    await expect(toggle).toBeVisible();
  });

  test('事项列表页：切到列表形态后 table 渲染', async ({ page }) => {
    // 测试目的：验证切换到「列表」形态后 URL 仍保持 /#/todos（切形态不改变路由）
    // 数据边界：切形态后 table 容器可见，但不要求有数据行（空表头也算通过）
    await page.goto(`${BASE}/#/todos`);
    await page.waitForTimeout(ROUTE_SETTLE_MS);

    // 点击「列表」按钮（Segmented 第二个选项）
    const toggle = page.getByTestId('todo-center-view-toggle');
    // Segmented 内部是两个 button，点 list 那个（title="列表"）
    await toggle.locator('button[title="列表"]').click();
    await page.waitForTimeout(ROUTE_SETTLE_MS);

    // URL 仍是列表（切形态不改变 URL）
    await expect(page).toHaveURL(/\/#\/todos$/);

    // Ant Design Table 应可见
    await expect(page.locator('.ant-table').first()).toBeVisible();
  });

  test('事项详情页：URL /#/todos/:id 刷新保持', async ({ page }) => {
    // 测试目的：验证详情页刷新后 URL 仍保持为 /#/todos/:id，确保用户刷新不会丢失当前查看的详情
    // 数据边界：使用不存在的 id=999999，验证应用在无数据时也不篡改 URL（应显示空态引导）
    // 直接构造一个事项详情 URL；即便 id 不存在，URL 也应保持不被改写
    // （应用会显示 Empty 引导但不会跳走）
    await page.goto(`${BASE}/#/todos/999999`);
    await page.waitForTimeout(ROUTE_SETTLE_MS);

    // 执行浏览器刷新：验证详情 URL 在刷新后仍保持
    await page.reload();
    await page.waitForTimeout(ROUTE_SETTLE_MS);

    // URL 保持详情路径
    await expect(page).toHaveURL(/\/#\/todos\/999999$/);
    // 页面应正常挂载（找到 LeftRail 或 PageCard 任一即认为未崩溃）
    await expect(page.locator('.ntd-left-rail-slot, .ntd-page-card').first()).toBeVisible();
  });

  test('环路列表页：URL /#/loops 直接进入可渲染', async ({ page }) => {
    // 测试目的：验证 /#/loops 列表页直接进入可正确渲染，URL 不被改写
    // 数据边界：不依赖 loop 数据，仅校验页面标题容器
    await page.goto(`${BASE}/#/loops`);
    await page.waitForTimeout(ROUTE_SETTLE_MS);

    await expect(page).toHaveURL(/\/#\/loops$/);

    // 环路列表标题应为「环路」
    await expect(page.locator('.ntd-page-card-title-text', { hasText: '环路' }).first()).toBeVisible();
  });

  test('环路详情页：URL /#/loops/:id 刷新保持', async ({ page }) => {
    // 测试目的：验证环路详情页刷新后 URL 仍保持 /#/loops/:id
    // 数据边界：使用不存在的 id=999999，验证无数据情况下应用不崩溃、URL 不被改写
    await page.goto(`${BASE}/#/loops/999999`);
    await page.waitForTimeout(ROUTE_SETTLE_MS);

    // 执行浏览器刷新：验证详情 URL 在刷新后仍保持
    await page.reload();
    await page.waitForTimeout(ROUTE_SETTLE_MS);

    await expect(page).toHaveURL(/\/#\/loops\/999999$/);
    await expect(page.locator('.ntd-left-rail-slot, .ntd-page-card').first()).toBeVisible();
  });

  test('旧 URL /#/items 不再做兼容重定向，应用不崩溃', async ({ page }) => {
    // 测试目的：验证 028 明确放弃旧 /#/items 兼容后，应用不会因访问旧 URL 而崩溃
    // 数据边界：旧 URL 落到 fallback 视图（/#/todos 或停留 /#/items 均视为失败），
    //          关键约束是应用容器 .ntd-left-rail-slot 仍可见
    // 028 明确不做旧 URL 兼容；访问旧 URL 时应用应落到 fallback 视图（不跳转、不崩溃）
    await page.goto(`${BASE}/#/items`);
    await page.waitForTimeout(ROUTE_SETTLE_MS);

    // 应用应正常挂载：LeftRail 容器存在
    // （body 在 React 挂载失败时也始终存在，不能证明应用未崩溃）
    await expect(page.locator('.ntd-left-rail-slot').first()).toBeVisible();

    // URL 不应被重定向到旧 /#/items 之外（fallback 到 /#/todos 也算正常）
    // 关键约束：不会停留在 /#/items 上假装渲染（无对应 View）
    const url = page.url();
    expect(url).not.toMatch(/\/#\/items$/);
  });

  test('事项列表 → 点击行跳转到 /#/todos/:id（如有数据）', async ({ page }) => {
    // 测试目的：验证列表形态下点击行能跳转到 /#/todos/:id 详情独立路由
    // 数据边界：列表无数据时 test.skip 跳过（不视为失败）；有数据时校验 URL 形如 /#/todos/<数字>
    await page.goto(`${BASE}/#/todos`);
    await page.waitForTimeout(ROUTE_SETTLE_MS);

    // 切到列表形态
    const toggle = page.getByTestId('todo-center-view-toggle');
    await toggle.locator('button[title="列表"]').click();
    await page.waitForTimeout(ROUTE_SETTLE_MS);

    // 找第一行数据行（排除表头）
    const firstRow = page.locator('.ant-table-tbody tr.ant-table-row').first();
    const rowCount = await page.locator('.ant-table-tbody tr.ant-table-row').count();

    // 没数据则跳过（不视为失败）—— 测试库可能为空
    test.skip(rowCount === 0, '事项列表无数据，跳过行点击测试');

    await firstRow.click();
    await page.waitForTimeout(ROUTE_SETTLE_MS);

    // URL 应跳到 /#/todos/<id>
    await expect(page).toHaveURL(/\/#\/todos\/\d+$/);
  });

  test('环路列表 → 点击行跳转到 /#/loops/:id（如有数据）', async ({ page }) => {
    // 测试目的：验证环路列表点击行能跳转到 /#/loops/:id 详情独立路由
    // 数据边界：列表无数据时 test.skip 跳过；有数据时校验 URL 形如 /#/loops/<数字>
    await page.goto(`${BASE}/#/loops`);
    await page.waitForTimeout(ROUTE_SETTLE_MS);

    const firstRow = page.locator('.ant-table-tbody tr.ant-table-row').first();
    const rowCount = await page.locator('.ant-table-tbody tr.ant-table-row').count();

    test.skip(rowCount === 0, '环路列表无数据，跳过行点击测试');

    await firstRow.click();
    await page.waitForTimeout(ROUTE_SETTLE_MS);

    await expect(page).toHaveURL(/\/#\/loops\/\d+$/);
  });

  test('详情页浏览器后退回到列表', async ({ page }) => {
    // 测试目的：验证从列表进入详情后，浏览器后退能回到 /#/todos 列表（pushUrl 写入历史生效）
    // 数据边界：列表无数据时 test.skip 跳过；有数据时校验后退后 URL 回到 /#/todos（无 id 段）
    await page.goto(`${BASE}/#/todos`);
    await page.waitForTimeout(ROUTE_SETTLE_MS);

    // 切到列表形态
    const toggle = page.getByTestId('todo-center-view-toggle');
    await toggle.locator('button[title="列表"]').click();
    await page.waitForTimeout(ROUTE_SETTLE_MS);

    const rowCount = await page.locator('.ant-table-tbody tr.ant-table-row').count();
    test.skip(rowCount === 0, '事项列表无数据，跳过后退测试');

    // 点击行进入详情
    await page.locator('.ant-table-tbody tr.ant-table-row').first().click();
    await page.waitForTimeout(ROUTE_SETTLE_MS);
    await expect(page).toHaveURL(/\/#\/todos\/\d+$/);

    // 浏览器后退
    await page.goBack();
    await page.waitForTimeout(ROUTE_SETTLE_MS);

    // 应回到列表 URL
    await expect(page).toHaveURL(/\/#\/todos$/);
  });

  test('事项卡片墙：点击卡片跳转到 /#/todos/:id（如有数据）', async ({ page }) => {
    // 测试目的：验证 028 改造后，卡片形态下点击卡片跳转到 /#/todos/:id 而非临时切列表
    // 数据边界：无卡片数据时 test.skip 跳过；有数据时校验 URL 变为 /#/todos/<数字>
    await page.goto(`${BASE}/#/todos`);
    await page.waitForTimeout(ROUTE_SETTLE_MS);

    // 确认在卡片形态（默认），找一张卡片
    const card = page.locator('[data-testid=todo-center-card]').first();
    const cardCount = await page.locator('[data-testid=todo-center-card]').count();

    test.skip(cardCount === 0, '卡片墙无数据，跳过卡片点击测试');

    await card.click();
    await page.waitForTimeout(ROUTE_SETTLE_MS);

    // URL 应变为 /#/todos/<id>（不再停留在列表页）
    await expect(page).toHaveURL(/\/#\/todos\/\d+$/);
  });

  test('批量操作：选中行后触发批量按钮可见', async ({ page }) => {
    // 测试目的：验证列表形态下选中行后批量操作按钮（更换执行器等）能通过 table
    // 行选择框触发显示
    // 数据边界：列表无数据时 test.skip 跳过；有数据时校验选中后批量按钮可见
    await page.goto(`${BASE}/#/todos`);
    await page.waitForTimeout(ROUTE_SETTLE_MS);

    // 切到列表形态
    const toggle = page.getByTestId('todo-center-view-toggle');
    await toggle.locator('button[title="列表"]').click();
    await page.waitForTimeout(ROUTE_SETTLE_MS);

    const rowCount = await page.locator('.ant-table-tbody tr.ant-table-row').count();
    test.skip(rowCount === 0, '事项列表无数据，跳过批量操作测试');

    // 勾选第一行复选框
    const checkbox = page.locator('.ant-table-tbody tr.ant-table-row').first().locator('.ant-checkbox-input');
    await checkbox.check({ force: true });
    await page.waitForTimeout(300);

    // 批量操作触发器按钮应可见
    const batchTrigger = page.getByTestId('todo-list-batch-trigger');
    await expect(batchTrigger).toBeVisible();
    // 文字应显示已选数量
    await expect(page.locator('text=已选 1 项')).toBeVisible();
  });
});
