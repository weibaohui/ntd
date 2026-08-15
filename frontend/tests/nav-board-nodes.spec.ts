// 030-导航关系图黑板看板节点 Playwright 功能测试。
// 对应测试文档 docs/testing/030-导航关系图黑板看板节点-测试.md 的 TC-01 到 TC-05。
// （TC-06 触发器 fallback 随 044-loop-slim 下线删除。）
// （TC-03/TC-05 看板节点用例随 097「看板视图归位事项菜单」下线：关系图的 kanban
//  观察节点已删（GRAPH_NODES 现为 10 个，无 kanban），无节点可断言；看板入口改由
//  事项页视图态承载，另见 031-tasks-time-range-filter 等事项页套件覆盖。）
// baseURL 见 playwright.config.ts：http://localhost:18088（dev embedded 模式）。
// 关系图为纯静态渲染，后端 API 不可达不影响本套件断言。

import { test, expect, type Page } from '@playwright/test';

/**
 * 每个用例前写入 onboarding 完成标记：
 * 防止「首登自动跳转 onboarding」逻辑（若存在）干扰 hash 断言，双保险。
 * 不做静默 try/catch：localStorage 写失败时让异常在初始化阶段直接抛，
 * 避免带着缺失标记继续执行、报错落到远离根因的后续断言上。
 */
async function presetStorage(page: Page) {
  await page.addInitScript(() => {
    localStorage.setItem('ntd_onboarding_completed', 'true');
  });
}

/** 进入导航页并滚到关系图 section（各用例公共前置，滚出视口的 SVG 不影响交互断言）。 */
async function gotoGraph(page: Page) {
  await page.goto('/#/onboarding');
  await page.getByRole('heading', { name: '概念关系图' }).scrollIntoViewIfNeeded();
}

/**
 * 读取指定节点的 circle fill 属性。
 * 选 fill attribute 而非计算样式：fill 是 React 受控 attribute，hover 状态同步落 DOM，
 * 比 getComputedStyle 少一层 CSS 解析不确定性；供 expect.poll 轮询使用。
 */
async function nodeFill(page: Page, nodeId: string): Promise<string | null> {
  return page
    .getByTestId(`onboarding-graph-node-${nodeId}`)
    .locator('circle')
    .evaluate((el) => el.getAttribute('fill'));
}

test.describe('030-导航关系图黑板看板节点', () => {
  test.beforeEach(async ({ page }) => {
    await presetStorage(page);
  });

  test('TC-01 关系图渲染 10 个节点，含黑板', async ({ page }) => {
    await gotoGraph(page);
    // 用 testid 而非文本定位：label 文本可能与其他 UI 撞车，testid 是节点专属锚点
    await expect(page.getByTestId('onboarding-graph-node-blackboard')).toBeVisible();
    // 每节点 1 个 <circle>，marker 箭头用的是 <path> 不是 circle。
    // 节点数变迁：044-loop-slim 删 trigger（12→11），097「看板归位事项」删 kanban（11→10）。
    // 断言圆总数与 GRAPH_NODES 常量保持同步，是节点增删的第一道回归闸门。
    const circles = page.getByTestId('onboarding-relation-graph').locator('svg circle');
    await expect(circles).toHaveCount(10);
  });

  test('TC-02 点击黑板 → Drawer 三层语义说明 + 跳转黑板页', async ({ page }) => {
    await gotoGraph(page);
    await page.getByTestId('onboarding-graph-node-blackboard').click();
    // Drawer 文案须含「事项」「执行记录」「环节」三关键词（需求 §5.3 三层语义全覆盖）
    const drawer = page.locator('.ant-drawer-open');
    await expect(drawer.getByText(/事项/)).toBeVisible();
    await expect(drawer.getByText(/执行记录/)).toBeVisible();
    await expect(drawer.getByText(/环节/)).toBeVisible();
    // 跳转按钮存在且文案正确：按钮是「先关 Drawer 再 pushUrl」的唯一触发点
    const gotoBtn = page.getByTestId('onboarding-graph-drawer-goto-blackboard');
    await expect(gotoBtn).toBeVisible();
    await expect(gotoBtn).toHaveText('去黑板页');
    await gotoBtn.click();
    // hash 断言全串匹配：blackboard 无 query 参数，/#/blackboard 是唯一合法结果
    await expect(page).toHaveURL(/#\/blackboard/);
    // 跳转后 onboarding 卸载，Drawer 必须随之消失 —— 专防「遮罩残留盖住新页面」回归
    await expect(page.locator('.ant-drawer-open')).toHaveCount(0);
  });

  // TC-03（点击看板节点 → Drawer + 跳转 kanban 模式）已随 097 下线：kanban 观察节点
  // 从 GRAPH_NODES 删除，看板入口归位事项页视图态，本套件无节点可断言。

  test('TC-04 hover 黑板 → 事项/执行记录/环路同步高亮 + 连线升级', async ({ page }) => {
    await gotoGraph(page);
    await page.getByTestId('onboarding-graph-node-blackboard').hover();
    // expect.poll 轮询到目标态而非固定 sleep：慢 CI 上 React 状态落 DOM 的耗时不确定，
    // 固定 300ms 可能读到旧值造成间歇性失败
    await expect.poll(() => nodeFill(page, 'todo')).toBe('#1677ff');
    await expect.poll(() => nodeFill(page, 'execution')).toBe('#1677ff');
    await expect.poll(() => nodeFill(page, 'loop')).toBe('#1677ff');
    // 无关节点（执行器）不应进入激活态：验证高亮列表没有过度扩散
    expect(await nodeFill(page, 'executor')).not.toBe('#1677ff');
    // 三条黑板连线 hover 后应从支线细灰（1.5/#d9d9d9）升级到主色加粗（3/#1677ff）
    // 断言 stroke + stroke-width 两个 attribute：Edge active 时两者同时变化，缺一不算生效
    for (const edgeId of ['todo-blackboard', 'execution-blackboard', 'loop-blackboard']) {
      const edge = page.getByTestId(`onboarding-graph-edge-${edgeId}`);
      await expect(edge).toHaveAttribute('stroke', '#1677ff');
      await expect(edge).toHaveAttribute('stroke-width', '3');
    }
  });

  // TC-05（hover 看板节点 → 执行记录高亮）已随 097 下线，同 TC-03 原因。

  // TC-06「回归：既有 fallback 节点（触发器）行为不变」已随 044-loop-slim 下线整体删除：
  // 044 移除触发能力时同步删掉了 onboarding-graph-node-trigger 节点（见 concepts.tsx GRAPH_NODES，
  // 注释「044：触发器节点已随触发能力下线移除」）。trigger 不再渲染，断言其 drawer 行为已无意义。
  // 黑板/看板仍走 GraphNodeDrawerFallback 分支，TC-02/TC-03 已覆盖 fallback+跳转路径的有效性。
});
