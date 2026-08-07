// 062-页面头部返回与标题统一：验证全部子页面返回按钮位置/样式/文案与标题格式。
//
// 设计文档：docs/design/062-页面头部返回与标题统一-设计.md
//
// 统一约定：
// 1. 返回按钮在页头 extra 区最右端（操作按钮之后），样式 ant-btn-sm + ant-btn-text + 左箭头；
// 2. 文案默认「返回列表」，目标非列表（帖子页→详情、Wiki→黑板）为「返回」；
// 3. 标题格式「模块名 #id: 具体名称」（无 id 的页面「模块名: 具体名称」）；
// 4. ProcessEditor 为自建 Toolbar，返回按钮同样位于操作区最右、样式对齐；
// 5. 移动端 MobileHeader 在 tasks 详情也显示返回按钮（此前遗漏）。
//
// 数据依赖：开发库（18088）需存在 todo#8 / loop#8 / task#32 / execution#46(属 todo#27) /
// wiki slug=code-quality-monitoring / 工艺 guid=4bafee67（E2E验证工艺055）。
// 缺数据时桌面端用例整体 skip（对齐 028 spec 先例），移动端用例不依赖数据照常运行。

import { test, expect, type Page } from '@playwright/test';

const BASE = 'http://localhost:18088';

/** 等待 hash 路由生效 + 详情数据加载的统一延迟。 */
const ROUTE_SETTLE_MS = 1500;

/** 种子数据探测结果：缺失时桌面端标题断言无意义，整体跳过（beforeAll 赋值）。 */
let seedOk = false;

// 桌面端标题断言依赖固定种子数据；参考 028 spec 先例，缺失时跳过而非失败。
// 通过 API 探测全部必需记录，任一条缺失即视为种子数据未就绪。
test.beforeAll(async ({ request }) => {
  try {
    const [todosRes, loopsRes, tasksRes, execsRes, wikiRes, procRes] = await Promise.all([
      request.get(`${BASE}/api/v1/workspaces/1/todos?page=1&limit=100`),
      request.get(`${BASE}/api/v1/workspaces/1/loops`),
      request.get(`${BASE}/api/v1/workspaces/1/tasks`),
      request.get(`${BASE}/api/v1/workspaces/1/executions?page=1&limit=100`),
      request.get(`${BASE}/api/v1/workspaces/1/wiki/files`),
      request.get(`${BASE}/api/bundled/processes?is_system=false`),
    ]);
    const [todos, loops, tasks, execs, wiki, procs] = await Promise.all([
      todosRes.json(), loopsRes.json(), tasksRes.json(), execsRes.json(), wikiRes.json(), procRes.json(),
    ]);
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const d = (j: any) => j?.data;
    seedOk =
      d(todos)?.items?.some((t: { id: number }) => t.id === 8) === true &&
      d(todos)?.items?.some((t: { id: number }) => t.id === 27) === true &&
      d(loops)?.some((l: { id: number }) => l.id === 8) === true &&
      d(tasks)?.some((t: { id: number }) => t.id === 32) === true &&
      d(execs)?.records?.some((r: { id: number; todo_id: number }) => r.id === 46 && r.todo_id === 27) === true &&
      d(wiki)?.some((w: { slug: string }) => w.slug === 'code-quality-monitoring') === true &&
      d(procs)?.some((p: { guid: string }) => p.guid === '4bafee67-a3e7-4c1f-b096-5f60ec8f6c14') === true;
  } catch {
    // 后端未启动或接口异常：视为无种子数据，桌面端用例跳过
    seedOk = false;
  }
});

/**
 * 断言 PageCard 头部：返回按钮位于 extra 区最右端，且为统一的 small+text 样式。
 * PageCard 将 onBack 按钮渲染为 .ntd-page-card-extra 的最后一个直接子元素。
 */
async function expectUnifiedBackButton(page: Page, label: string) {
  const backBtn = page.locator('.ntd-page-card-extra > button:last-child');
  await expect(backBtn).toBeVisible();
  await expect(backBtn).toContainText(label);
  // 统一样式：small + text（062 约定，参见 PageCard.onBack 实现）
  await expect(backBtn).toHaveClass(/ant-btn-sm/);
  await expect(backBtn).toHaveClass(/ant-btn-text/);
}

test.describe('062 页面头部返回与标题统一（桌面端）', () => {
  // 种子数据未就绪时跳过桌面端全部用例（标题内容断言依赖固定记录）
  test.beforeEach(() => {
    test.skip(!seedOk, '开发库缺少 062 所需种子数据（todo#8/loop#8/task#32 等），跳过桌面端用例');
  });

  test('事项详情页：标题「事项 #id: 标题」+ 返回列表在 extra 最右端', async ({ page }) => {
    await page.goto(`${BASE}/#/todos/8`);
    await page.waitForTimeout(ROUTE_SETTLE_MS);

    // 标题格式：模块名 #id: 具体标题
    await expect(page.locator('.ntd-page-card-title-text').first()).toContainText('事项 #8: ');
    // 返回按钮在 extra 最右端（操作按钮之后），文案「返回列表」
    await expectUnifiedBackButton(page, '返回列表');
  });

  test('环路详情页：标题含环路名称 + 返回列表在 extra 最右端', async ({ page }) => {
    await page.goto(`${BASE}/#/loops/8`);
    await page.waitForTimeout(ROUTE_SETTLE_MS);

    // 062 新增：环路名称上提到 PageCard 标题（hideTitleRow 后内层不再展示名称）
    await expect(page.locator('.ntd-page-card-title-text').first()).toContainText('环路 #8: FEAT042-loop');
    await expectUnifiedBackButton(page, '返回列表');
  });

  test('任务详情页：标题「任务 #id: 标题」+ 返回列表在 extra 最右端', async ({ page }) => {
    await page.goto(`${BASE}/#/tasks/32`);
    await page.waitForTimeout(ROUTE_SETTLE_MS);

    await expect(page.locator('.ntd-page-card-title-text').first()).toContainText('任务 #32: ');
    await expectUnifiedBackButton(page, '返回列表');
  });

  test('帖子页：标题带事项前缀 + 返回（非返回列表）在 extra 最右端', async ({ page }) => {
    await page.goto(`${BASE}/#/todos/27/posts/46`);
    await page.waitForTimeout(ROUTE_SETTLE_MS);

    // 标题与事项详情页同格式（此前只有裸标题，丢了模块名与 id）
    await expect(page.locator('.ntd-page-card-title-text').first()).toContainText('事项 #27: ');
    // 返回目标是父级详情页而非列表，文案为「返回」
    await expectUnifiedBackButton(page, '返回');
  });

  test('Wiki 页：标题「Wiki: slug」+ antd 返回按钮（原生 button 已移除）', async ({ page }) => {
    await page.goto(`${BASE}/#/wiki?workspace=1&slug=code-quality-monitoring`);
    await page.waitForTimeout(ROUTE_SETTLE_MS);

    await expect(page.locator('.ntd-page-card-title-text').first()).toHaveText('Wiki: code-quality-monitoring');
    // 统一后返回按钮是 PageCard 渲染的 antd Button（不再是手写内联样式的原生 button）
    await expectUnifiedBackButton(page, '返回');
  });

  test('环路配置页：标题「环路配置: 工作空间名」+ 返回列表在 extra（不再占用 icon 位）', async ({ page }) => {
    // 配置页无独立 URL，从环路列表页「配置」按钮进入
    await page.goto(`${BASE}/#/loops`);
    await page.waitForTimeout(ROUTE_SETTLE_MS);
    await page.getByRole('button', { name: '配置' }).click();
    await page.waitForTimeout(ROUTE_SETTLE_MS);

    await expect(page.locator('.ntd-page-card-title-text').first()).toContainText('环路配置: ');
    await expectUnifiedBackButton(page, '返回列表');
  });

  test('工艺编辑器：标题「工艺: 显示名」+ 返回列表在操作区最右、样式对齐', async ({ page }) => {
    await page.goto(`${BASE}/#/processes?guid=4bafee67-a3e7-4c1f-b096-5f60ec8f6c14&processMode=edit`);
    await page.waitForTimeout(ROUTE_SETTLE_MS);

    // 标题格式统一（此前为「显示名 (唯一名)」，无模块名前缀）
    await expect(page.locator('text=工艺: E2E验证工艺055').first()).toBeVisible();

    // 返回按钮：操作区（antd Space）最后一个按钮，文案「返回列表」，样式 small+text
    const backBtn = page.getByRole('button', { name: '返回列表' });
    await expect(backBtn).toBeVisible();
    await expect(backBtn).toHaveClass(/ant-btn-sm/);
    await expect(backBtn).toHaveClass(/ant-btn-text/);
    // 位置断言：返回按钮是其父容器（操作区 Space）的最后一个按钮
    const isLast = await backBtn.evaluate((el) => {
      const parent = el.parentElement;
      if (!parent) return false;
      const buttons = parent.querySelectorAll('button');
      return buttons[buttons.length - 1] === el;
    });
    expect(isLast).toBe(true);
  });
});

test.describe('062 页面头部（移动端）', () => {
  // 移动端视口：触发 useIsMobile 分支，渲染 MobileHeader
  test.use({ viewport: { width: 375, height: 812 } });

  test('tasks 详情：MobileHeader 显示返回按钮（062 补齐）', async ({ page }) => {
    await page.goto(`${BASE}/#/tasks/32`);
    await page.waitForTimeout(ROUTE_SETTLE_MS);

    // 062 前 tasks 详情在移动端无返回入口；现与 todos/loops 一致
    await expect(page.locator('.mobile-header-menu-btn[aria-label="返回列表"]')).toBeVisible();
  });

  test('todos 详情：MobileHeader 返回按钮保持可用（回归）', async ({ page }) => {
    await page.goto(`${BASE}/#/todos/8`);
    await page.waitForTimeout(ROUTE_SETTLE_MS);

    await expect(page.locator('.mobile-header-menu-btn[aria-label="返回列表"]')).toBeVisible();
  });

  test('tasks 详情：点击 MobileHeader 返回后真正回到任务列表（CodeRabbit#7 回归）', async ({ page }) => {
    // CodeRabbit 曾担忧：backToList 走 replaceUrl 不触发 popstate，TasksPage 内部
    // selectedTaskId 可能残留导致卡在详情态。实际上 URL 带任务 id 时 App 渲染的是
    // TaskDetailPage（TasksPage 已整体卸载，内部状态不存在），返回后 TasksPage 重新挂载
    // 并从 URL 初始化为列表态。本用例点击验证该行为，防止未来路由改动破坏此前提。
    // 注意：本用例只依赖 URL 路由态，不依赖种子数据，故不受 seedOk 门控。
    await page.goto(`${BASE}/#/tasks/32`);
    await page.waitForTimeout(ROUTE_SETTLE_MS);

    await page.locator('.mobile-header-menu-btn[aria-label="返回列表"]').click();
    await page.waitForTimeout(ROUTE_SETTLE_MS);

    // 断言 URL 回到任务列表，且列表页 PageCard 标题「任务」可见（未卡在详情态）
    await expect(page).toHaveURL(/\/#\/tasks$/);
    await expect(page.locator('.ntd-page-card-title-text', { hasText: '任务' }).first()).toBeVisible();
  });
});
