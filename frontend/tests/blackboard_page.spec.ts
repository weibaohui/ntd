// 验证 BlackBoardPage 的核心行为：
// 1. Markdown 渲染：h1 / list / link 都能正常显示
// 2. ntd://todo/{id} 内部链接点击后通过 useViewState.selectTodo 导航到 items 视图
// 3. 刷新按钮触发 POST /api/.../blackboard/refresh
// 4. workspace 切换时重新拉取（修复 useState 快照 bug）
//
// 测试策略：使用 page.route() 拦截后端 API 返回固定 JSON，
// 避免依赖真实 LLM 写入。

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

interface BlackboardResponse {
  id: number;
  workspace_id: number;
  content: string;
  updated_at: string | null;
}

function makeResponse(workspaceId: number, content: string): BlackboardResponse {
  return {
    id: content ? 1 : 0,
    workspace_id: workspaceId,
    content,
    updated_at: content ? '2026-07-03T10:00:00Z' : null,
  };
}

/** 安装 mock：根据 query 中的 workspace 返回不同内容 */
async function installBlackboardMocks(page: Page) {
  // 拦截 GET blackboard：返回当前 workspace 对应的内容
  await page.route('**/api/v1/workspaces/*/blackboard', async (route) => {
    const url = new URL(route.request().url());
    // 解析 workspace id：取 path 中数字段
    const m = url.pathname.match(/\/api\/workspaces\/(\d+)\/blackboard/);
    const wsId = m ? Number(m[1]) : 0;
    // 用独立的 SAMPLE_CONTENT_WS2（不仅文字不同，URL 也不同），避免 href 漂移
    const content = wsId === 2 ? SAMPLE_CONTENT_WS2 : SAMPLE_CONTENT;
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({ code: 0, data: makeResponse(wsId, content), message: 'ok' }),
    });
  });
  // 拦截 POST refresh：返回成功
  await page.route('**/api/v1/workspaces/*/blackboard/refresh', async (route) => {
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({ code: 0, data: { success: true, message: '黑板刷新已触发' }, message: 'ok' }),
    });
  });
}

test('黑板页面渲染 Markdown：h1 / list / link 都能显示', async ({ page }) => {
  await installBlackboardMocks(page);
  await page.goto(`${BACKEND_URL}/?view=blackboard`);
  await page.waitForTimeout(1000);

  // 标题
  const h1 = page.locator('h1', { hasText: '工作空间进展' });
  await expect(h1).toBeVisible();

  // 子标题
  const h2 = page.locator('h2', { hasText: '已确认' });
  await expect(h2).toBeVisible();

  // 列表项
  const item = page.locator('li', { hasText: /关键结论见/ });
  await expect(item).toBeVisible();

  // 内部链接渲染为可点击元素
  const internalLink = page.locator('a[href*="/items?id=42"]');
  await expect(internalLink).toBeVisible();
});

test('ntd://todo/42 内部链接点击后导航到 items 视图并选中对应 todo', async ({ page }) => {
  await installBlackboardMocks(page);
  await page.goto(`${BACKEND_URL}/?view=blackboard`);
  await page.waitForTimeout(1000);

  // 点击内部链接
  const link = page.locator('a[href*="/items?id=42"]').first();
  await link.click();
  await page.waitForTimeout(500);

  // 验证 URL 已更新到 items?id=42
  expect(page.url()).toContain('view=items');
  expect(page.url()).toContain('id=42');
});

test('点击刷新按钮触发 POST /api/.../blackboard/refresh', async ({ page }) => {
  await installBlackboardMocks(page);

  // 记录 refresh 请求次数
  let refreshCalls = 0;
  await page.route('**/api/v1/workspaces/*/blackboard/refresh', async (route) => {
    refreshCalls += 1;
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({ code: 0, data: { success: true, message: '黑板刷新已触发' }, message: 'ok' }),
    });
  });

  await page.goto(`${BACKEND_URL}/?view=blackboard`);
  await page.waitForTimeout(1000);

  // 刷新按钮：先等 button 可点击（loading 结束）
  const refreshButton = page.locator('button', { hasText: '刷新' });
  await expect(refreshButton).toBeEnabled();
  await refreshButton.click();
  await page.waitForTimeout(500);

  expect(refreshCalls).toBeGreaterThanOrEqual(1);
});

test('切换 workspace 后页面重新拉取并渲染新内容（修复 useState 快照 bug）', async ({ page }) => {
  await installBlackboardMocks(page);

  // 默认 workspace=1，先打开黑板
  await page.goto(`${BACKEND_URL}/?view=blackboard`);
  await page.waitForTimeout(1000);
  // 验证初始内容（todo_42）
  const link42 = page.locator('a[href*="/items?id=42"]');
  await expect(link42).toBeVisible();

  // 通过 URL 切换到 workspace=2，触发 prop 或 URL 变化
  await page.goto(`${BACKEND_URL}/?view=blackboard&workspace=2`);
  await page.waitForTimeout(1500);

  // 验证新内容（todo_77）出现，旧内容不再可见
  const link77 = page.locator('a[href*="/items?id=77"]');
  await expect(link77).toBeVisible();
  // todo_42 是 workspace=1 特有的；切换后不应当还显示
  // （注：playwright locator 仍可能匹配上 DOM 节点，需要等 fetch 完成；这里给 1.5s 缓冲）
  const count42 = await page.locator('a[href*="/items?id=42"]').count();
  expect(count42).toBe(0);
});

test('空内容时显示空状态文案', async ({ page }) => {
  // 覆盖 mock 返回空内容
  await page.route('**/api/v1/workspaces/*/blackboard', async (route) => {
    const m = new URL(route.request().url()).pathname.match(/\/api\/workspaces\/(\d+)\//);
    const wsId = m ? Number(m[1]) : 0;
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({ code: 0, data: makeResponse(wsId, ''), message: 'ok' }),
    });
  });
  await page.goto(`${BACKEND_URL}/?view=blackboard`);
  await page.waitForTimeout(1000);

  // 验证空状态文案
  await expect(page.getByText('暂无内容')).toBeVisible();
  await expect(page.getByText('任务执行后将自动更新黑板内容')).toBeVisible();

  // 空状态下刷新按钮被禁用（避免无意义的 LLM 调用）
  const refreshButton = page.locator('button', { hasText: '刷新' });
  await expect(refreshButton).toBeDisabled();
});
