import { test, expect } from '@playwright/test';

// 验证手机端「事项」页 header 的「新建」按钮可见。
// 修复前：ItemsPage 在移动端构建的 extra 只有 Segmented、没有「新建」，
// 该 extra 下发到 TodoCenterCardView / TodoMobilePage（均无自备新建），
// 导致手机端事项页找不到创建入口；而「环路」页 LoopMobilePage 自带新建，故正常。
const BASE = 'http://localhost:18088';

// 手机视口（iPhone X 尺寸），触发 useIsMobile(阈值 768) 的手机分支。
test.use({ viewport: { width: 375, height: 812 } });

test('手机端事项页 header 的「新建」按钮可见且可打开创建弹窗', async ({ page }) => {
  test.setTimeout(60000);
  // 直接进入事项列表（默认 panel=list）。事项卡片/列表由后端 bundled 提供，稍等加载。
  await page.goto(`${BASE}/#/items`);
  await page.waitForTimeout(2000);

  // 定位 header extra 区里的「新建」按钮。修复前该区域只有 Segmented，此按钮不存在。
  const createBtn = page
    .locator('.ntd-page-card-extra')
    .getByRole('button', { name: '新建' });
  await expect(createBtn, '手机端事项页 header 应有「新建」按钮').toBeVisible({ timeout: 10000 });

  // 截图留档（目录已 gitignore）：手机端事项页 header 的「新建」按钮清晰可见。
  await page.screenshot({ path: 'tests/__screenshots__/mobile-todo-create-header.png' });

  // 点击应打开创建事项的抽屉（onOpenCreateModal → setTodoModalOpen → TodoDrawer），
  // 证明按钮确实接到了创建逻辑，而非只是一个空壳。
  await createBtn.click();
  await expect(page.locator('.ant-drawer'), '点击「新建」应打开创建抽屉').toBeVisible({ timeout: 8000 });
  // 截图留档（目录已 gitignore），便于在 PR 里附图说明手机端效果。
  await page.screenshot({ path: 'tests/__screenshots__/mobile-todo-create.png' });
});
