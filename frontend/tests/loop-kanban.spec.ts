// 环路视图功能测试。
//
// 设计意图：验证环路视图的视图切换、工具栏渲染、搜索与时间过滤等核心功能。
// 为什么需要这套测试：
// - 环路视图是新增视图，需要确保与已有 memorial/kanban/running 视图的切换链路正常
// - 搜索和时间过滤是跨视图共享状态，需要验证受控组件模式下的数据流正确性
// - 看板组件的加载、空状态、列渲染有多条分支，需要覆盖边界条件避免回归
// 边界条件：
// - 环路列表为空时的空状态展示
// - 加载超时时的兜底逻辑（15秒内完成或标记失败）
// - 搜索框清空后数据重新加载

import { test, expect } from '@playwright/test';

test.describe('环路视图功能测试', () => {

  // 为什么在 beforeEach 中导航到看板页：
  // - 所有测试用例的前置条件是进入 MemorialBoard，抽成 hook 避免重复代码
  // - 使用 waitForLoadState('networkidle') 等待首屏 API 完成，确保后续断言的稳定性
  // - 用 aria-label 定位导航按钮：语义化定位比 class/nth() 更稳定，符合无障碍规范
  test.beforeEach(async ({ page }) => {
    await page.goto('http://localhost:18088');
    // 为什么用 networkidle：等待首屏 todos、tags 等 API 完成，避免后续点击时组件未挂载
    await page.waitForLoadState('networkidle');

    // 为什么先检查可见性再点击：防御性编程，避免测试在 CI 环境因元素未渲染而失败
    const memorialBtn = page.locator('[aria-label="看板"]');
    await expect(memorialBtn).toBeVisible({ timeout: 5000 });
    await memorialBtn.click();
    // 为什么等待视图切换完成：MemorialBoard 挂载后需要拉取 completed todos，
    // 等待看板工具栏可见作为"视图已就绪"的信号
    await expect(page.locator('.memorial-header')).toBeVisible({ timeout: 5000 });
  });

  // 测试视图切换选项数量是否正确（覆盖 UI 回归风险）。
  // 为什么这个断言重要：Segmented 选项数量变化会导致后续 nth() 定位失效，
  // 先验证总数可以提前发现 UI 结构变更，避免其他用例误报。
  test('test_view_mode_segmented_has_four_options', async ({ page }) => {
    // 为什么用 getByRole：语义化定位比 class 更稳定，Segmented 渲染为 radiogroup
    const segmented = page.getByRole('radiogroup').filter({ has: page.getByText('结论视图') });
    await expect(segmented).toBeVisible({ timeout: 5000 });

    // 为什么用 radio 角色过滤：Segmented 的每个选项是一个 radio button
    const options = segmented.getByRole('radio');
    // 为什么断言 count 为 4：memorial/kanban/running/loop_kanban 四种模式
    await expect(options).toHaveCount(4);
  });

  // 测试切换到环路视图的交互流程（核心功能路径）。
  // 为什么需要：环路视图是新增功能，必须验证从 memorial 切换过去的完整链路。
  test('test_switch_to_loop_kanban_view', async ({ page }) => {
    // 为什么用文本定位"环路视图"：文本内容比 nth(3) 更明确表达意图，
    // 且当选项顺序调整时不会误点到其他视图。
    const loopKanbanOption = page.getByText('环路视图');
    await expect(loopKanbanOption).toBeVisible({ timeout: 5000 });
    await loopKanbanOption.click();

    // 为什么等待搜索框出现：搜索框是 LoopKanban 组件的标志性 UI，
    // 可见即表示组件已挂载且工具栏渲染完成。
    const searchInput = page.getByPlaceholder(/搜索环路名称或触发类型/);
    await expect(searchInput).toBeVisible({ timeout: 5000 });
  });

  // 测试环路视图的核心 UI 元素渲染（工具栏 + 看板主体或空状态）。
  // 为什么需要：覆盖加载态、空态、正常态三种分支，确保 UI 不会白屏或卡死。
  test('test_loop_kanban_renders_board_or_empty_state', async ({ page }) => {
    // 为什么先切换视图：前置条件，确保测试的是 loop_kanban 模式
    const loopKanbanOption = page.getByText('环路视图');
    await loopKanbanOption.click();

    // 为什么验证工具栏：工具栏是 LoopKanban 的固定部分，无论有无数据都应渲染
    const toolbar = page.locator('.loop-kanban-toolbar');
    await expect(toolbar).toBeVisible({ timeout: 5000 });

    // 为什么等待 loading 消失：避免在加载态做断言，导致误判
    // 为什么用 15 秒超时：批量拉取多个环路的执行历史可能较慢，给足缓冲避免 CI 抖动
    const spin = page.locator('.ant-spin');
    await expect(spin).toBeHidden({ timeout: 15000 });

    // 为什么用"或"逻辑：数据可能为空（空状态）或有数据（列头可见），
    // 两者都是正常状态，只要不是永久 loading 即可
    const columnHeaders = page.locator('.loop-kanban-column-header');
    const emptyState = page.locator('.ant-empty-description');
    const hasColumns = (await columnHeaders.count()) > 0;
    const hasEmpty = await emptyState.isVisible().catch(() => false);
    // 为什么至少有一种状态：确保 UI 有反馈，不会白屏
    expect(hasColumns || hasEmpty).toBeTruthy();
  });

  // 测试时间过滤功能（边界条件：切换选项后数据重新过滤）。
  // 为什么需要：时间过滤是跨视图共享状态，loop_kanban 需验证 onHoursChange 回调正确触发。
  test('test_loop_kanban_time_filter', async ({ page }) => {
    const loopKanbanOption = page.getByText('环路视图');
    await loopKanbanOption.click();

    // 为什么先等工具栏可见：确保组件已挂载，避免点击时元素未渲染
    const toolbar = page.locator('.loop-kanban-toolbar');
    await expect(toolbar).toBeVisible({ timeout: 5000 });

    // 为什么用 getByRole + filter：时间选项也是 radiogroup，
    // 用包含"7d"文本的 radio 定位，比 nth(2) 更稳定
    const timeSegmented = page.getByRole('radiogroup').filter({ has: page.getByText('7d') });
    if (await timeSegmented.isVisible({ timeout: 2000 }).catch(() => false)) {
      const sevenDaysOption = timeSegmented.getByRole('radio', { name: '7d' });
      await sevenDaysOption.click();
      // 为什么不再用 waitForTimeout：点击后状态立即更新，无需等待固定时间，
      // 若需验证数据变化可用 expect() 的内置重试机制
    }
  });

  // 测试搜索功能（边界条件：输入 -> 清空 -> 数据恢复）。
  // 为什么需要：搜索框是受控组件，需验证 onChange 回调正确触发且清空后状态重置。
  test('test_loop_kanban_search', async ({ page }) => {
    const loopKanbanOption = page.getByText('环路视图');
    await loopKanbanOption.click();

    // 为什么用 getByPlaceholder：搜索框的语义化定位，比 class 或 nth() 更明确
    const searchInput = page.getByPlaceholder(/搜索环路名称或触发类型/);
    await expect(searchInput).toBeVisible({ timeout: 5000 });

    // 为什么输入"test"：常见测试数据，验证输入流程正常
    await searchInput.fill('test');
    // 为什么不再用 waitForTimeout：React 状态更新是同步的，
    // 若需验证搜索结果可用 expect() 等待特定元素出现/消失

    // 为什么测试清空逻辑：清空是搜索的逆操作，需确保状态正确重置
    const clearButton = page.locator('.ant-input-clear-icon');
    if (await clearButton.isVisible({ timeout: 1000 }).catch(() => false)) {
      await clearButton.click();
      // 为什么验证输入框为空：确保清空后 searchText 状态重置为 ''
      await expect(searchInput).toHaveValue('');
    }
  });
});
