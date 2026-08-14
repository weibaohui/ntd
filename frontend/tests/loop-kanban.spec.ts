// 环路执行历史看板功能测试。
//
// 设计意图：验证 /#/loops 页的列表/看板视图切换、工具栏渲染、搜索与时间过滤等核心功能。
//
// 为什么需要这套测试：
// - 097/098 重构后环路看板归位到 /#/loops 页内 Segmented 切换（list 定义 table /
//   kanban 执行历史），不再是 MemorialBoard 的四视图之一——本套件验证新的切换链路。
// - 搜索和时间过滤是跨视图共享状态（同一 headerExtra 下推给 LoopKanban），
//   需要验证受控组件模式下的数据流正确性。
// - 看板组件的加载、空状态、列渲染有多条分支，需要覆盖边界条件避免回归。
//
// 边界条件：
// - 执行历史为空时的空状态展示
// - 搜索框清空后数据重新加载
//
// 前置说明：未钉 selected_workspace —— 本套件所有断言（视图切换/工具栏/空态）不依赖
// 具体 workspace 的数据内容，任何 ws 下列头与空态必居其一，故无需钉定。

import { test, expect } from '@playwright/test';

test.describe('环路视图功能测试', () => {

  // 为什么在 beforeEach 中直接进 /#/loops：
  // 098 后环路是左侧导航独立入口（aria-label="环路"），直达 hash 路由比逐级点导航更稳；
  // networkidle 等首屏 loops API 完成，避免后续点击时 Segmented 未挂载。
  test.beforeEach(async ({ page }) => {
    await page.goto('http://localhost:18088/#/loops');
    await page.waitForLoadState('networkidle');

    // 等视图切换器可见作为「页面已就绪」的信号（data-testid 是专属锚点，
    // 不受 antd Segmented 内部 DOM 结构调整影响）。
    await expect(page.getByTestId('loop-list-view-toggle')).toBeVisible({ timeout: 5000 });
  });

  // 测试视图切换选项数量是否正确（覆盖 UI 回归风险）。
  // 为什么这个断言重要：Segmented 选项数量变化会导致后续定位失效，
  // 先验证总数可以提前发现 UI 结构变更，避免其他用例误报。
  test('test_view_mode_segmented_has_two_options', async ({ page }) => {
    // 为什么用 getByRole：语义化定位比 class 更稳定，Segmented 渲染为 radiogroup
    const segmented = page.getByTestId('loop-list-view-toggle');
    const options = segmented.getByRole('radio');
    // 为什么断言 count 为 2：list（定义 table）/ kanban（执行历史看板）两种模式。
    // 097/098 前的 memorial/kanban/running/loop_kanban 四视图已随运行中心拆解下线。
    await expect(options).toHaveCount(2);
  });

  // 测试切换到环路看板的交互流程（核心功能路径）。
  // 为什么需要：看板是执行历史的观察入口，必须验证从 list 切过去的完整链路。
  test('test_switch_to_loop_kanban_view', async ({ page }) => {
    // 为什么用 filter(hasText)：antd Segmented 选项的可见 label 渲染在 item 内部
    // （radio input 是视觉隐藏的，直接点 radio 角色会「not visible」超时）；
    // hasText 过滤后点整个 item，语义明确且不依赖 nth 顺序。
    await page
      .getByTestId('loop-list-view-toggle')
      .locator('.ant-segmented-item-label[title="看板"]')
      .click();

    // 为什么等 .loop-kanban-board：它是 LoopKanban 根 div，恒定渲染，
    // 可见即表示组件已挂载（工具栏 headerExtra 在两个视图下都渲染，不能作切换信号）。
    await expect(page.locator('.loop-kanban-board')).toBeVisible({ timeout: 5000 });
  });

  // 测试环路看板的核心 UI 元素渲染（列头 + 看板主体或空状态）。
  // 为什么需要：覆盖加载态、空态、正常态三种分支，确保 UI 不会白屏或卡死。
  test('test_loop_kanban_renders_board_or_empty_state', async ({ page }) => {
    await page
      .getByTestId('loop-list-view-toggle')
      .locator('.ant-segmented-item-label[title="看板"]')
      .click();

    // .loop-kanban-board 恒定渲染，作为「组件已挂载」的可靠信号（见上用例注释）。
    await expect(page.locator('.loop-kanban-board')).toBeVisible({ timeout: 10000 });

    // 为什么用 .or() + toBeVisible 而非一次性 count/isVisible：
    // useLoopExecutions 有两段加载（先环路列表、再批量执行历史），中间 loading 会二次置 true，
    // 一次性读取可能正好撞上 loading 把 columns/empty 替换成 Spin 的瞬间。
    // Playwright 的 expect(...).toBeVisible() 会自动重试到目标稳定可见，给足 15s 覆盖两段加载。
    // 数据为空 → 空状态；有数据 → 列头；二者必居其一，只要不永久 loading 即通过。
    const columns = page.locator('.loop-kanban-column-header');
    const emptyState = page.locator('.ant-empty-description');
    await expect(columns.or(emptyState).first()).toBeVisible({ timeout: 15000 });
  });

  // 测试时间过滤功能（边界条件：切换选项后数据重新过滤）。
  // 为什么需要：时间窗是 kanban 态共享状态，需验证 onHoursChange 回调正确触发。
  test('test_loop_kanban_time_filter', async ({ page }) => {
    await page
      .getByTestId('loop-list-view-toggle')
      .locator('.ant-segmented-item-label[title="看板"]')
      .click();
    await expect(page.locator('.loop-kanban-board')).toBeVisible({ timeout: 5000 });

    // 时间窗 TimeRangeSegmented 仅 kanban 态渲染；用「7d」文本定位比 nth(2) 更稳定。
    const timeOption = page.getByText('7d', { exact: true });
    await expect(timeOption).toBeVisible({ timeout: 5000 });
    await timeOption.click();
    // 点击后无崩溃信号即可：数据集变化依赖 dev 库内容，不做数量断言避免环境敏感。
    await expect(page.locator('.loop-kanban-board')).toBeVisible({ timeout: 5000 });
  });

  // 测试搜索功能（边界条件：输入 -> 清空 -> 数据恢复）。
  // 为什么需要：搜索框是受控组件（LoopListHeader 持有状态下推 LoopKanban），
  // 需验证 onChange 回调正确触发且清空后状态重置。
  test('test_loop_kanban_search', async ({ page }) => {
    // 搜索框在 headerExtra 中，两个视图共享，无需先切看板即可验证。
    // 为什么用 getByPlaceholder：语义化定位，比 class 或 nth() 更明确。
    const searchInput = page.getByPlaceholder('搜索环路名称');
    await expect(searchInput).toBeVisible({ timeout: 5000 });

    // 为什么输入"test"：常见测试数据，验证输入流程正常。
    await searchInput.fill('test');
    await expect(searchInput).toHaveValue('test');

    // 为什么测试清空逻辑：清空是搜索的逆操作，需确保状态正确重置。
    const clearButton = page.locator('.ant-input-clear-icon');
    if (await clearButton.isVisible({ timeout: 1000 }).catch(() => false)) {
      await clearButton.click();
      // 为什么验证输入框为空：确保清空后 searchKeyword 状态重置为 ''。
      await expect(searchInput).toHaveValue('');
    }
  });
});
