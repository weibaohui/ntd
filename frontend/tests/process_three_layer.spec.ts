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
  // 预选中工作空间：SPA 仅在启动时读一次 selected_workspace，必须用 addInitScript
  // 在首屏挂载前写入 localStorage；旧写法（goto 后 evaluate）已太晚，导致安装按钮被禁用。
  await page.addInitScript(() => localStorage.setItem('selected_workspace', '1'));

  // ── 步骤 1（G3）：工艺页安装第一个工艺，验证自动跳转到新环路详情 ──
  await page.goto('http://localhost:18088/#/processes');
  await page.waitForLoadState('networkidle');

  // 数据前置：dev 库为共享 SQLite，「我的」视图里的非系统工艺 install 行为不可控
  // （历史 fixture 或用户手建条目都可能 install 失败，不产生新环路，后续链路全断）。
  // 先探测系统模板是否可安装，不可用则跳过（避免在他人的 dev 环境上误报）。
  const probe = await page.request.get('http://localhost:18088/api/bundled/processes?is_system=true');
  const probeBody = await probe.json().catch(() => null);
  const sysCount = Array.isArray(probeBody?.data) ? probeBody.data.length : 0;
  test.skip(sysCount === 0, 'dev 库无系统工艺模板，无法验证安装链路');

  // 切到「模板」scope：系统模板的 guid 为真实 bundled UUID，install 必成功并产生新环路；
  // 留在「我的」可能命中行为不可控的非系统条目，install 不保证成功。
  await page.getByText('模板', { exact: true }).click();

  // 等工艺卡片渲染后点第一个「安装」按钮
  const installBtn = page.getByRole('button', { name: /安装/ }).first();
  await expect(installBtn).toBeVisible({ timeout: 8000 });
  await installBtn.click();
  // 确认安装 Modal（okText="安装"）。Modal 打开时卡片上的「安装」按钮仍在 DOM 中，
  // 全局按 name 取会命中两个 → strict mode；scope 到 dialog 内只取 Modal 的 OK 按钮。
  // antd v6 起 Button 对两个汉字自动插空格（autoInsertSpace），okText="安装" 的无障碍名变成
  // 「安 装」而非「安装」；用正则 /^安\s*装$/ 兼容「安装」「安 装」两种渲染，避免 v6 升级后失配。
  const confirmBtn = page.getByRole('dialog').getByRole('button', { name: /^安\s*装$/ });
  await expect(confirmBtn).toBeVisible({ timeout: 5000 });
  await confirmBtn.click();
  // 028 起环路详情用 path 段路由 /#/loops/<id>（旧的 ?id= 已废弃）。
  await page.waitForURL(/#\/loops\/\d+/, { timeout: 15000 });
  // 从 URL path 段末尾取出新环路 id，供后续步骤回跳用。
  const loopId = page.url().match(/#\/loops\/(\d+)/)?.[1];
  expect(loopId).toBeTruthy();

  // ── 步骤 2（G4）：环路详情展示「来源工艺」行 ──
  const sourceRow = page.locator('[data-testid="loop-source-process"]');
  await expect(sourceRow).toBeVisible({ timeout: 10000 });
  await expect(sourceRow).toContainText('来源工艺');

  // ── 步骤 3（G4）：点击来源工艺行 → 跳工艺页且详情 Modal 自动打开 ──
  // TraceBreadcrumb（P2 重构）把可点击区域收窄到内层 label <span>：外层 loop-source-process
  // div 的几何中心落在「来源工艺：」标题/图标上，Playwright 默认中心点击命中标题而非链接，
  // onClick 不触发 → 不跳转。精确点内层带 cursor:pointer 的 label span 才能触发回跳。
  const sourceLabel = sourceRow.locator('[style*="cursor: pointer"]').first();
  await expect(sourceLabel).toBeVisible({ timeout: 5000 });
  await sourceLabel.click();
  // 040 起工艺按 guid 寻址（name 可重复），URL 变为 /#/processes?guid=<guid>。
  await page.waitForURL(/#\/processes\?guid=/, { timeout: 8000 });
  // 详情 Modal 自动打开，默认落在「流程图」Tab（旧版「原始定义」YAML 区块已改为 Tab 形态）。
  await expect(page.locator('.ant-tabs-tab').filter({ hasText: '流程图' })).toBeVisible({ timeout: 10000 });
  // 不需要显式关闭 Modal：下一步 page.goto 切到环路详情视图，ProcessPage 卸载、Modal 随之销毁。
  // （zhCN locale 下 Modal 右上 X 的 aria-label 也是「关闭」，与 footer 按钮同名会触发 strict mode。）

  // ── 步骤 4（G5）：回环路详情，点流程图节点上的事项标题 → 跳事项详情 ──
  // 028：环路详情即 /#/loops/<id>，不再用 ?panel=detail 区分。
  await page.goto(`http://localhost:18088/#/loops/${loopId}`);
  await page.waitForLoadState('networkidle');
  const todoLink = page.locator('[data-testid^="flow-todo-link-"]').first();
  await expect(todoLink).toBeVisible({ timeout: 10000 });
  await todoLink.click();
  // 028：事项详情路由统一为 /#/todos/<id>（旧 /#/items?id= 已废弃）。
  await page.waitForURL(/#\/todos\/\d+/, { timeout: 8000 });

  // ── 步骤 5（G6）：事项详情展示「所属环路」区块，点击跳回环路详情 ──
  const refSection = page.locator('[data-testid="todo-referencing-loops"]');
  await expect(refSection).toBeVisible({ timeout: 10000 });
  await expect(refSection).toContainText('所属环路');
  await refSection.locator('.ant-tag').first().click();
  await page.waitForURL(/#\/loops\/\d+/, { timeout: 8000 });
});

test('P1-工艺详情三 Tab：流程图、实例环路、YAML 源', async ({ page }) => {
  // 同上：用 addInitScript 在 SPA 首屏挂载前写入工作空间，否则工艺列表不加载、详情按钮不渲染。
  await page.addInitScript(() => localStorage.setItem('selected_workspace', '1'));
  await page.goto('http://localhost:18088/#/processes');
  await page.waitForLoadState('networkidle');

  // 数据前置：dev 库为共享 SQLite，「我的」视图里的非系统工艺 getProcess 可能失败
  // （setDetail 不触发 → 详情 Modal 不开）。先探测系统模板是否可用，不可用则跳过
  // （避免在他人的 dev 环境上误报）。系统模板 guid 为真实 bundled UUID，可稳定寻址。
  const probe = await page.request.get('http://localhost:18088/api/bundled/processes?is_system=true');
  const probeBody = await probe.json().catch(() => null);
  const sysCount = Array.isArray(probeBody?.data) ? probeBody.data.length : 0;
  test.skip(sysCount === 0, 'dev 库无系统工艺模板，无法验证详情 Modal');

  // 切到「模板」scope：系统模板的 guid 是 bundled 真实 UUID，getProcess 必返 200；
  // 留在「我的」可能命中非系统条目导致 Modal 打不开（见上）。
  await page.getByText('模板', { exact: true }).click();

  // 点第一个工艺卡片上的「详情」按钮打开 Modal
  const detailBtn = page.getByRole('button', { name: /详情/ }).first();
  await expect(detailBtn).toBeVisible({ timeout: 8000 });
  await detailBtn.click();

  // Modal 加载后，默认应展示「流程图」Tab——验证流程图区域出现
  // （ProcessFlowGraph 渲染 SVG，空的工艺会显示「该工艺定义无法解析」或
  // 「暂无环节定义」，至少其中一种存在即为通过）
  await expect(page.locator('.ant-tabs-tab').filter({ hasText: '流程图' })).toBeVisible({ timeout: 8000 });

  // 切换到「实例环路」Tab
  await page.locator('.ant-tabs-tab').filter({ hasText: '实例环路' }).click();
  // 实例环路 Tab 内容出现（至少显示 Empty 提示或表格）
  await expect(page.getByText(/尚未安装|状态|打开/).first()).toBeVisible({ timeout: 6000 });

  // 切换到「YAML 源」Tab
  await page.locator('.ant-tabs-tab').filter({ hasText: 'YAML 源' }).click();
  // 关键字 process: 或 limits: 应出现（原始 YAML 正文）
  await expect(page.getByText('process:', { exact: false }).or(page.getByText('limits:', { exact: false }))).toBeVisible({ timeout: 6000 });
});
