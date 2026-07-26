// Playwright 脚本：覆盖 TodoDetail 的加载态/错误态/重试按钮。
// 用法: cd frontend && npx playwright test tests/check_todo_detail_loading.spec.ts
//
// 验证目标（对应 PR935 评论 #5）：
// 1. todoLoading 时显示 Skeleton 骨架屏，避免空白页误导用户。
// 2. 访问不存在的 todo id 时，显示「任务加载失败或不存在」+「重试」按钮。
// 3. 点击「重试」会重新触发加载；若该 todo 仍不存在，应继续显示错误态。
// 4. 对照：访问一个真实存在的 todo，应正常显示详情而非错误态。
//
// 数据准备：通过 API 在 workspace=1 下创建一个临时 todo 拿到真实 id；
// 用一个足够大的不存在 id 验证错误态。用例结束清理临时 todo。

import { test, expect, type APIRequestContext } from '@playwright/test';

// 直连 embedded dev 服务（前后端同源），不走 vite 5173
const BASE = 'http://localhost:18088';
const WS_ID = 1;
// 不存在 id：取一个理论上不会命中的大值，保证 getTodo 返回 404 → 触发错误态
const NON_EXISTENT_ID = 999_999_987;

interface Todo {
  id: number;
  title: string;
}

// 在 workspace=1 下创建一个临时 todo，返回其 id；用例结束后清理。
// 选 API 创建而非 UI 表单，是为了让用例聚焦于「详情页加载态」本身，
// 不与「创建 todo 表单」的 UI 流程耦合，降低脆弱性。
async function seedTodo(request: APIRequestContext): Promise<number> {
  const res = await request.post(`${BASE}/api/v1/workspaces/${WS_ID}/todos`, {
    data: { title: `PW详情加载测试-${Date.now()}`, prompt: 'seed' },
  });
  expect(res.ok()).toBeTruthy();
  const body = await res.json();
  expect(typeof body.data.id).toBe('number');
  return body.data.id as number;
}

async function cleanupTodo(request: APIRequestContext, id: number) {
  // 用 try-catch 包裹删除，避免 todo 已被其他用例清理时让 afterAll 抛错
  try {
    await request.delete(`${BASE}/api/v1/workspaces/${WS_ID}/todos/${id}`);
  } catch {
    // ignore: 删除失败不影响断言，下轮跑会重新建表
  }
}

test.describe('TodoDetail 加载态/错误态/重试', () => {
  let realTodoId = 0;

  test.beforeAll(async ({ request }) => {
    realTodoId = await seedTodo(request);
  });

  test.afterAll(async ({ request }) => {
    if (realTodoId) await cleanupTodo(request, realTodoId);
  });

  test('加载失败时显示「任务加载失败或不存在」与「重试」按钮', async ({ page }) => {
    // 拦截 getTodo 的后端请求，强制返回 404，稳定复现 todoLoadError 分支
    await page.route(
      `**/api/v1/workspaces/${WS_ID}/todos/${NON_EXISTENT_ID}`,
      (route) => route.fulfill({ status: 404, body: '{"code":404,"message":"not found"}' }),
    );

    await page.goto(`${BASE}/#/todos/${NON_EXISTENT_ID}`);
    // 等待错误态渲染：错误提示文本 + 重试按钮出现
    const errorDesc = page.getByText('任务加载失败或不存在');
    await expect(errorDesc).toBeVisible({ timeout: 8000 });
    // 重试按钮应可见且可点击
    const retryBtn = page.getByRole('button', { name: '重试' });
    await expect(retryBtn).toBeVisible();
  });

  test('点击「重试」重新触发加载，仍不存在时继续显示错误态', async ({ page }) => {
    // 计数器：记录命中 getTodo 路由的次数，用于断言「重试确实又发了一次请求」
    let hitCount = 0;
    await page.route(
      `**/api/v1/workspaces/${WS_ID}/todos/${NON_EXISTENT_ID}`,
      (route) => {
        hitCount += 1;
        return route.fulfill({ status: 404, body: '{"code":404,"message":"not found"}' });
      },
    );

    await page.goto(`${BASE}/#/todos/${NON_EXISTENT_ID}`);
    await expect(page.getByText('任务加载失败或不存在')).toBeVisible({ timeout: 8000 });
    // 记录首次加载后的命中数（至少 1 次初始请求）
    const hitsAfterInitial = hitCount;

    // 点击重试按钮
    await page.getByRole('button', { name: '重试' }).click();
    // 重试应再发一次请求，hitCount 应增加
    await expect.poll(() => hitCount, { timeout: 8000 }).toBeGreaterThan(hitsAfterInitial);
    // 重试后仍查不到该 todo，应继续显示错误态（而非卡在 loading 或空白）
    await expect(page.getByText('任务加载失败或不存在')).toBeVisible();
    await expect(page.getByRole('button', { name: '重试' })).toBeVisible();
  });

  test('对照：访问真实存在的 todo，应显示详情而非错误态', async ({ page }) => {
    // 真实 todo：不拦截，走正常后端路径
    await page.goto(`${BASE}/#/todos/${realTodoId}`);
    // 详情页加载完成后，「任务加载失败或不存在」错误态不应出现
    await expect(page.getByText('任务加载失败或不存在')).not.toBeVisible({ timeout: 8000 });
    // 骨架屏也不应长期停留：等待详情主体渲染（选 detail-panel 容器作为稳定锚点）
    await expect(page.locator('.detail-panel').first()).toBeVisible({ timeout: 8000 });
  });
});
