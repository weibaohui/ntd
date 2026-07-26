import { test, expect } from '@playwright/test';

/**
 * 027 批次 1（P0）端到端验证：工艺 → 环路 → 事项 三层链路闭环。
 *
 * 覆盖需求 G3/G4/G5/G6：
 * - G3 安装工艺成功后自动跳转新环路详情
 * - G4 环路详情展示「来源工艺」行，点击回跳工艺详情（URL 带 name 参数自动开 Modal）
 * - G5 环路流程图节点上的事项标题可点击跳事项详情
 * - G6 事项详情展示「所属环路」区块，点击跳回环路详情
 *
 * 前置条件：
 * - dev 服务运行在 18088（make dev），库中已同步工艺模板
 * - 存在 id=1 的工作空间（dev 库默认「临时工作空间」）
 *
 * 测试会真实安装一个工艺实例（产生新环路 + 新事项），属幂等可重复操作。
 */
test('三层链路闭环：工艺→环路→事项→环路', async ({ page }) => {
  // 预选中工作空间（useTodoContext 从 localStorage 恢复选择），否则安装按钮被禁用
  await page.goto('http://localhost:18088');
  await page.evaluate(() => localStorage.setItem('selected_workspace', '1'));

  // ── 步骤 1（G3）：工艺页安装第一个工艺，验证自动跳转到新环路详情 ──
  await page.goto('http://localhost:18088/#/processes');
  await page.waitForLoadState('networkidle');
  // 等工艺卡片渲染后点第一个「安装」按钮
  const installBtn = page.getByRole('button', { name: /安装/ }).first();
  await expect(installBtn).toBeVisible({ timeout: 8000 });
  await installBtn.click();
  // 确认安装 Modal
  const confirmBtn = page.getByRole('button', { name: '安装', exact: true });
  await expect(confirmBtn).toBeVisible({ timeout: 5000 });
  await confirmBtn.click();
  // 安装成功后 URL 应自动跳到 /#/loops?id=<新 id>
  await page.waitForURL(/#\/loops\?id=\d+/, { timeout: 15000 });
  const loopUrl = page.url();
  const loopId = new URLSearchParams(loopUrl.split('?')[1]).get('id');
  expect(loopId).toBeTruthy();

  // ── 步骤 2（G4）：环路详情展示「来源工艺」行 ──
  const sourceRow = page.locator('[data-testid="loop-source-process"]');
  await expect(sourceRow).toBeVisible({ timeout: 10000 });
  await expect(sourceRow).toContainText('来源工艺：');

  // ── 步骤 3（G4）：点击来源工艺行 → 跳工艺页且详情 Modal 自动打开 ──
  await sourceRow.click();
  await page.waitForURL(/#\/processes\?name=/, { timeout: 8000 });
  // 详情 Modal 自动打开（含「原始定义」YAML 区块即为详情内容）
  await expect(page.getByText('原始定义')).toBeVisible({ timeout: 10000 });
  // 关闭 Modal，避免遮挡后续操作
  await page.getByRole('button', { name: '关闭' }).click();

  // ── 步骤 4（G5）：回环路详情，点流程图节点上的事项标题 → 跳事项详情 ──
  await page.goto(`http://localhost:18088/#/loops?id=${loopId}&panel=detail`);
  await page.waitForLoadState('networkidle');
  const todoLink = page.locator('[data-testid^="flow-todo-link-"]').first();
  await expect(todoLink).toBeVisible({ timeout: 10000 });
  await todoLink.click();
  await page.waitForURL(/#\/items\?id=\d+/, { timeout: 8000 });

  // ── 步骤 5（G6）：事项详情展示「所属环路」区块，点击跳回环路详情 ──
  const refSection = page.locator('[data-testid="todo-referencing-loops"]');
  await expect(refSection).toBeVisible({ timeout: 10000 });
  await expect(refSection).toContainText('所属环路：');
  await refSection.locator('.ant-tag').first().click();
  await page.waitForURL(/#\/loops\?id=\d+/, { timeout: 8000 });
});
