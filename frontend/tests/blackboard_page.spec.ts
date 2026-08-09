// 验证 BlackboardPage 的核心行为（Wiki 化重构后）：
// 1. Markdown 渲染：h1 / list / link 都能正常显示
// 2. ntd://todo/{id} 内部链接点击后通过 useViewState.selectTodo 导航到事项详情
// 3. 刷新按钮重新拉取页面列表（GET 重新请求，不再是 POST /blackboard/refresh）
// 4. workspace 切换时重新拉取并渲染新内容（修复 useState 快照 bug）
// 5. 单个页面内容为空时显示空状态文案
//
// 重构背景：黑板从「单内容 blob（GET /blackboard 返回 content 字符串）」改为
// 「Wiki 文件树」——内容来源变为 GET /wiki/files（页面列表）+ GET /wiki/files/{slug}
//（页面正文）。ntd://todo/{id} 内链渲染为 <a href="#/todos/{id}"> 并 onClick 调
// selectTodo。详见 src/components/BlackboardPage.tsx。
//
// 测试策略：用 page.route() 拦截 wiki 接口返回固定 JSON，避免依赖真实 LLM 写入。

import { test, expect, Page } from '@playwright/test';

const BACKEND_URL = 'http://localhost:18088';

const SAMPLE_CONTENT = [
  '# 工作空间进展',
  '',
  '## 已确认',
  '',
  '- 关键结论见 [todo_42](ntd://todo/42)',
  '- 文档位置：[/docs/spec.md](ntd://todo/99)',
  '',
  '## 下一步建议',
  '',
  '- 继续完成 [todo_100](ntd://todo/100)',
  '',
].join('\n');

const SAMPLE_CONTENT_WS2 = [
  '# 工作空间进展',
  '',
  '## 已确认',
  '',
  '- 关键结论见 [todo_77](ntd://todo/77)',
  '- 文档位置：[/docs/spec.md](ntd://todo/88)',
  '',
  '## 下一步建议',
  '',
  '- 继续完成 [todo_200](ntd://todo/200)',
  '',
].join('\n');

// 每个工作空间对应的页面正文：key=workspaceId。
const CONTENT_BY_WS: Record<number, string> = {
  1: SAMPLE_CONTENT,
  2: SAMPLE_CONTENT_WS2,
};

// Wiki 化后内容来自 topic 页面；列表里固定一个 test-topic，正文按 workspace 区分。
const TOPIC_SLUG = 'test-topic';

/**
 * 安装 wiki 接口 mock：
 * - GET /wiki/files          → 页面列表（一个 topic）
 * - GET /wiki/files/{slug}   → 页面正文（按 workspace 取 CONTENT_BY_WS）
 * - GET /blackboard          → 黑板配置（设置弹窗用，这里给空对象兜底）
 *
 * refreshCountRef（可选）：每命中一次列表请求就 +1，供刷新用例断言「重新拉取」。
 */
async function installWikiMocks(page: Page, refreshCountRef?: { count: number }) {
  // 列表接口：必须先注册，否则会被更宽的 file-content 通配吞掉（Playwright 后注册优先）。
  await page.route('**/api/v1/workspaces/*/wiki/files', async (route) => {
    if (refreshCountRef) refreshCountRef.count += 1;
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      // 列表项只需 slug + file_type（BlackboardPage 用 file_type 选默认 topic 页）。
      body: JSON.stringify({
        code: 0,
        data: [{ slug: TOPIC_SLUG, file_type: 'topic' }],
        message: 'ok',
      }),
    });
  });

  // 单页正文接口：从 URL 解析 workspaceId，返回对应工作空间的内容。
  // workspaceId===1 用 SAMPLE_CONTENT（含 todo_42），其余工作空间用 SAMPLE_CONTENT_WS2
  //（含 todo_77）——这样无论切换到哪个非 1 空间都能验证「内容随 workspace 变化」。
  await page.route('**/api/v1/workspaces/*/wiki/files/*', async (route) => {
    const m = new URL(route.request().url()).pathname.match(/\/workspaces\/(\d+)\/wiki\/files\//);
    const wsId = m ? Number(m[1]) : 1;
    const content = wsId === 1 ? SAMPLE_CONTENT : SAMPLE_CONTENT_WS2;
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        code: 0,
        data: { slug: TOPIC_SLUG, content },
        message: 'ok',
      }),
    });
  });

  // 配置接口：设置弹窗读取，空对象即可（fetchConfig 失败会被静默捕获）。
  await page.route('**/api/v1/workspaces/*/blackboard', async (route) => {
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({ code: 0, data: {}, message: 'ok' }),
    });
  });
}

test('黑板页面渲染 Markdown：h1 / list / link 都能显示', async ({ page }) => {
  await installWikiMocks(page);
  // app 默认锁到 dirs[0]（后端按 path 排序，dev 库首项非 ws1）；mock 内容按 workspace 区分
  //（ws1→todo_42，其余→todo_77），必须先钉 ws1，否则首屏落 ws3 拿到 WS2 内容，todo_42 永不出现。
  await page.addInitScript(() => {
    localStorage.setItem('selected_workspace', '1');
  });
  await page.goto(`${BACKEND_URL}/#/blackboard`);
  await page.waitForTimeout(1500);

  // 标题
  const h1 = page.locator('h1', { hasText: '工作空间进展' });
  await expect(h1).toBeVisible({ timeout: 10000 });

  // 子标题
  const h2 = page.locator('h2', { hasText: '已确认' });
  await expect(h2).toBeVisible();

  // 列表项
  const item = page.locator('li', { hasText: /关键结论见/ });
  await expect(item).toBeVisible();

  // 内部链接：ntd://todo/42 渲染为 <a href="#/todos/42">（TodoLink 转换协议）。
  // LazyXMarkdown 异步处理链接（先出 h1/list，链接稍后），给足超时避免 5s 默认值误判。
  const internalLink = page.locator('a[href*="#/todos/42"]');
  await expect(internalLink).toBeVisible({ timeout: 15000 });
});

test('ntd://todo/42 内部链接点击后导航到事项详情', async ({ page }) => {
  await installWikiMocks(page);
  // 同「渲染 Markdown」用例：钉 ws1 让 mock 返回 todo_42 内容，否则首屏落 ws3 拿到 todo_77。
  await page.addInitScript(() => {
    localStorage.setItem('selected_workspace', '1');
  });
  await page.goto(`${BACKEND_URL}/#/blackboard`);
  await page.waitForTimeout(1500);

  // 点击内部链接：onClick 调 selectTodo(42)，把 hash 切到 #/todos/42。
  // LazyXMarkdown 异步渲染链接，给 15s 超时避免冷启动时链接晚于 h1 出现导致误判。
  const link = page.locator('a[href*="#/todos/42"]').first();
  await expect(link).toBeVisible({ timeout: 15000 });
  await link.click();
  await page.waitForTimeout(500);

  // URL 应切到事项详情（#/todos/42）
  expect(page.url()).toMatch(/#\/todos\/42/);
});

test('点击刷新按钮重新拉取页面列表（GET 重取，非 POST refresh）', async ({ page }) => {
  // 重构后刷新=重新 GET 列表 + 正文，旧的 POST /blackboard/refresh 已移除。
  // 用列表请求计数断言「重新拉取」确实发生。
  const refreshCountRef = { count: 0 };
  await installWikiMocks(page, refreshCountRef);

  await page.goto(`${BACKEND_URL}/#/blackboard`);
  await page.waitForTimeout(1500);

  // 首屏已拉取过一次列表
  const initialCount = refreshCountRef.count;
  expect(initialCount).toBeGreaterThanOrEqual(1);

  // 桌面端标题栏右侧「刷新」按钮
  const refreshButton = page.getByRole('button', { name: '刷新' });
  await expect(refreshButton).toBeEnabled();
  await refreshButton.click();
  await page.waitForTimeout(800);

  // 点击后列表应被重新拉取（计数增加）
  expect(refreshCountRef.count).toBeGreaterThan(initialCount);
});

test('切换 workspace 后页面重新拉取并渲染新内容（修复 useState 快照 bug）', async ({ page }) => {
  await installWikiMocks(page);

  // app 默认锁到 dirs[0]（后端按 path 排序，dev 库首项是 ws3），初始内容不可预测；
  // 用 addInitScript 在 SPA 挂载前置 selected_workspace=1，确保黑板首屏落在 ws1（mock 返回 todo_42）。
  await page.addInitScript(() => {
    localStorage.setItem('selected_workspace', '1');
  });
  await page.goto(`${BACKEND_URL}/#/blackboard`);
  // 验证初始内容（ws1 → todo_42）；LazyXMarkdown 异步渲染链接，给足超时。
  const link42 = page.locator('a[href*="#/todos/42"]');
  await expect(link42).toBeVisible({ timeout: 15000 });

  // workspace 来自全局 app 状态，需通过左上角 WorkspaceSwitcher 切换。
  // 先用 API 找一个 id≠1 的工作空间名（mock 对 ws≠1 返回 WS2 内容 todo_77），
  // 避免盲取 nth(1)——固定 ws1 后该项恰好是 ws1 自己，切换无效。
  const dirsResp = await page.request.get(`${BACKEND_URL}/api/v1/project-directories`);
  const dirs: Array<{ id: number; name: string }> = (await dirsResp.json()).data || [];
  const target = dirs.find((d) => d.id !== 1);
  test.skip(!target, 'dev 库不足 2 个工作空间，跳过 workspace 切换验证');

  await page.getByRole('button', { name: '切换工作空间' }).click();
  await page.getByRole('menuitem', { name: (target as { name: string }).name }).click();

  // 切换后内容应重取（验证 useState 快照 bug 已修）：todo_77 出现、todo_42 消失。
  const link77 = page.locator('a[href*="#/todos/77"]');
  await expect(link77).toBeVisible({ timeout: 15000 });
  await expect(page.locator('a[href*="#/todos/42"]')).toHaveCount(0);
});

test('页面内容为空时显示空状态文案', async ({ page }) => {
  // 覆盖正文接口：topic 页存在但正文为空（区别于「无页面」的「暂无页面」）
  await page.route('**/api/v1/workspaces/*/wiki/files/*', async (route) => {
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({ code: 0, data: { slug: TOPIC_SLUG, content: '' }, message: 'ok' }),
    });
  });
  await page.route('**/api/v1/workspaces/*/wiki/files', async (route) => {
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({ code: 0, data: [{ slug: TOPIC_SLUG, file_type: 'topic' }], message: 'ok' }),
    });
  });
  await page.route('**/api/v1/workspaces/*/blackboard', async (route) => {
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({ code: 0, data: {}, message: 'ok' }),
    });
  });
  await page.goto(`${BACKEND_URL}/#/blackboard`);
  await page.waitForTimeout(1500);

  // 验证空状态文案（BlackboardEmpty：正文为空时渲染）
  await expect(page.getByText('暂无内容')).toBeVisible({ timeout: 10000 });
  await expect(page.getByText('任务执行后将自动更新黑板内容')).toBeVisible();
  // 注：刷新按钮在空内容态仍可用（不再因空内容禁用），故不断言 disabled。
});
