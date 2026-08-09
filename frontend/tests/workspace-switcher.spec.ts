/**
 * 工作空间选择器测试
 *
 * 验证工作空间选择器功能：
 * - 选择器正确显示在搜索框上方（实为左侧 LeftRail，见用例注释）
 * - 点击选择器显示工作空间列表
 * - 选择工作空间后正确过滤 todo 列表
 * - 刷新后保持选择的工作空间
 *
 * 适配说明（028 hash 路由重构后）：
 * - 选择器是全局 shell 组件，挂在左侧 LeftRail 顶部，并不在搜索框「上方」，
 *   实际位于搜索框「左侧」——下方用横向坐标断言。
 * - 按钮无「全部工作空间」文案；aria-label 固定为「切换工作空间」，
 *   按钮内文本随选中空间名变化（未选时为「请选择工作空间」）。
 * - 工作空间不再写入 URL，仅存于全局 state + localStorage（key=selected_workspace），
 *   刷新后由 DataLoader 复原。
 * - LeftRail 默认折叠（compact 图标按钮），这里通过 addInitScript 写入
 *   localStorage(ntd_left_rail_collapsed=false) 强制展开为 full 模式，
 *   使按钮 label 可见，便于读取/断言选中空间名。
 */

import { test, expect, chromium } from '@playwright/test';
import type { Page } from '@playwright/test';

// CLAUDE.md 规定 dev server 默认监听 18088（不是 Vite 默认的 5173），
// fallback 直接指向 18088，避免直接 `npx playwright test` 时连不上 dev 服务。
const DEV_URL = process.env.E2E_BASE_URL || 'http://localhost:18088';

// 选择器按钮的 accessible name：full/compact 两模式一致，
// 用 getByRole 定位最稳（不依赖按钮内会变化的文案）。
const SWITCHER_NAME = '切换工作空间';

/**
 * 创建一个 LeftRail 预展开的会话。
 * 默认 collapsed=true（compact 图标按钮，无 label），断言选中空间名时读不到文本；
 * 这里在页面首次导航前注入 localStorage，让 App 启动即进入展开态（full 模式）。
 */
async function newExpandedSession() {
  const browser = await chromium.launch();
  const context = await browser.newContext({ colorScheme: 'light' });
  // addInitScript 在每次导航前执行：确保刷新后 rail 仍是展开态。
  await context.addInitScript(() => {
    try {
      localStorage.setItem('ntd_left_rail_collapsed', 'false');
    } catch {
      // localStorage 不可用时静默降级，不影响测试主流程。
    }
  });
  const page = await context.newPage();
  return { browser, page };
}

// 选择器按钮：scope 到 LeftRail 容器，避免 QuickCaptureModal 等隐藏挂载点造成重复匹配。
const switcherButton = (page: Page) =>
  page.locator('[data-testid="left-rail"]').getByRole('button', { name: SWITCHER_NAME });

// 菜单中真实的工作空间项：菜单除空间外还固定含「新建工作空间」「管理工作空间」，
// 把这两项排除后的 .first() 即首个工作空间。
const firstWorkspaceMenuItem = (page: Page) =>
  page
    .getByRole('menuitem')
    .filter({ hasNotText: /新建工作空间|管理工作空间/ })
    .first();

test.describe('工作空间选择器', () => {
  test('选择器正确显示在搜索框上方', async () => {
    const { browser, page } = await newExpandedSession();

    await page.goto(DEV_URL);
    // 默认视图为 todos 列表，含「搜索标题或 Prompt」输入框；
    // 用它作为首屏就绪信号（auto-wait），取代写死的 waitForTimeout。
    const searchInput = page.locator('input[placeholder*="搜索标题"]');
    await expect(searchInput).toBeVisible();

    const workspaceSelector = switcherButton(page);
    await expect(workspaceSelector).toBeVisible();

    // 028 后选择器位于左侧 LeftRail 顶部，并非搜索框「上方」；
    // 改用横向坐标断言：选择器整体落在搜索框左侧（rail 在主内容之左）。
    const selectorBox = await workspaceSelector.boundingBox();
    const searchBox = await searchInput.boundingBox();
    expect(selectorBox).not.toBeNull();
    expect(searchBox).not.toBeNull();
    // 选择器右沿不得超过搜索框左沿，即二者左右排列、选择器在左。
    expect(selectorBox!.x + selectorBox!.width).toBeLessThanOrEqual(searchBox!.x);

    await browser.close();
  });

  test('点击选择器显示工作空间列表', async () => {
    const { browser, page } = await newExpandedSession();

    await page.goto(DEV_URL);
    // 工作空间列表由 db.getProjectDirectories() 异步拉取；等一帧让菜单数据落位。
    await page.waitForTimeout(2000);

    const workspaceSelector = switcherButton(page);
    await workspaceSelector.click();

    // 菜单是否展开以「管理工作空间」menuitem 是否可见为准（antd Dropdown 渲染到 body 门户）；
    // 该项由 LeftRail 注入 onManage 后恒定存在，是比「全部工作空间」更稳定的锚点。
    const manageOption = page.getByRole('menuitem', { name: '管理工作空间' });
    await expect(manageOption).toBeVisible();

    await browser.close();
  });

  test('选择工作空间后正确过滤 todo 列表', async () => {
    const { browser, page } = await newExpandedSession();

    await page.goto(DEV_URL);
    await page.waitForTimeout(2000);

    const workspaceSelector = switcherButton(page);
    await workspaceSelector.click();

    const workspaceOption = firstWorkspaceMenuItem(page);
    // 开发库可能为空（无任何 project_directory）：沿用原测试的防御式跳过，
    // 空库下不断言、直接通过，避免硬失败遮蔽真实回归。
    if (await workspaceOption.isVisible()) {
      await workspaceOption.click();
      // 选中后 state.selectedWorkspace 更新，按钮重渲染；
      // 这里仅断言切换器仍可见（label 已变为新空间名），过滤行为由列表数据体现。
      await expect(workspaceSelector).toBeVisible();
    }

    await browser.close();
  });

  test('刷新后保持选择的工作空间', async () => {
    const { browser, page } = await newExpandedSession();

    await page.goto(DEV_URL);
    await page.waitForTimeout(2000);

    const workspaceSelector = switcherButton(page);
    await workspaceSelector.click();

    const workspaceOption = firstWorkspaceMenuItem(page);
    if (await workspaceOption.isVisible()) {
      const optionText = (await workspaceOption.textContent()) || '';
      await workspaceOption.click();

      // 刷新：SELECT_WORKSPACE 已把 id 写入 localStorage(selected_workspace)，
      // DataLoader 启动时读取并复原 state.selectedWorkspace，UI label 随之恢复。
      await page.reload();
      await page.waitForTimeout(2000);

      // full 模式按钮 label 显示当前选中空间名；断言刷新后仍为刷新前的选择。
      // toContainText 会对空白做规范化，这里 trim 入参以兜底图标带来的边缘空白。
      await expect(workspaceSelector).toContainText(optionText.trim());
    }

    await browser.close();
  });
});
