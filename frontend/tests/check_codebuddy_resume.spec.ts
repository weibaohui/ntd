// Issue 058：验证 CodeBuddy 执行记录出现「继续对话」回复输入框，atomcode 不出现。
//
// 验证逻辑（基于 src/utils/executors.tsx 的 RESUMABLE_EXECUTORS）：
// - codebuddy ∈ RESUMABLE_EXECUTORS → 满足 status≠running + 有 session_id 时渲染 ReplyRow
// - atomcode ∉ RESUMABLE_EXECUTORS → 即使有 session_id 也不渲染
// （placeholder 文案「输入回复内容...」定义在 src/components/todo-detail/ReplyInput.tsx）
//
// 数据依赖：用例依赖 dev 库 ws1 下存在可用记录——
//   - 一条 codebuddy（success + 带 session_id，会话内最好仅该一条，保证 thread group 末尾即 codebuddy）
//   - 一条 atomcode（任意状态，atomcode 不可 resume 故 session_id 可有可无）
// 历史版本曾硬编码 record 40/41，但 dev 库中这两条已变成 todo 1 下的 pi 执行记录，
// 硬编码 ID 会随数据漂移失效。这里改为运行时探活：先打 executions 列表，client-side 过滤，
// 命中即用真实 id/todo_id 导航；缺数据则 test.skip 而非硬失败，避免误报。
import { test, expect } from '@playwright/test';

const BASE = 'http://localhost:18088';
// 工作空间固定为 1：本用例所有记录都从 ws1 取，selected_workspace 必须与记录归属一致，
// 否则帖子页按 ws 限域请求会 403/空态，ReplyRow 不渲染。
const WORKSPACE_ID = '1';

/** executions 列表返回的记录形状（仅列出用到的字段）。 */
interface ExecutionRecordLike {
  id: number;
  todo_id: number;
  executor: string;
  status: string;
  session_id?: string | null;
}

test('codebuddy 执行记录显示继续对话输入框，atomcode 不显示', async ({ page, request }) => {
  // 探活：拉取 ws1 最近一页执行记录（后端 limit 上限 100），client-side 过滤可用记录。
  // 后端 executions 列表不支持 executor 过滤参数，只能在客户端筛选。
  const resp = await request.get(
    `${BASE}/api/v1/workspaces/${WORKSPACE_ID}/executions?limit=100`,
  );
  const body = await resp.json();
  const records: ExecutionRecordLike[] = body?.data?.records ?? [];

  // 正向：codebuddy + success + 带 session_id（ReplyRow 渲染三条件，见 supportsResume）。
  const codebuddy = records.find(
    (r) => r.executor === 'codebuddy' && r.status === 'success' && !!r.session_id,
  );
  // 反向：任意 atomcode 记录（atomcode 不可 resume，无论有无 session_id 都不应出现输入框）。
  const atomcode = records.find((r) => r.executor === 'atomcode');

  // 数据未就绪时跳过而非失败：dev 库内容随开发滚动，硬编码 ID 注定漂移。
  test.skip(
    !codebuddy || !atomcode,
    `dev 库 ws1 缺少可用记录（codebuddy=${!!codebuddy}, atomcode=${!!atomcode}），跳过`,
  );

  // 帖子页按全局选中的工作空间拉取执行记录。SPA 仅在启动时读一次 selected_workspace，
  // 用 addInitScript 在首屏挂载前写入 localStorage，避免深链冷启动 ws=null 导致请求被跳过。
  await page.addInitScript((ws) => localStorage.setItem('selected_workspace', ws), WORKSPACE_ID);

  // 正向：codebuddy 记录的帖子页应出现「继续对话」回复输入框。
  await page.goto(`${BASE}/#/todos/${codebuddy!.todo_id}/posts/${codebuddy!.id}`);
  // 帖子页是 lazy chunk + 两步 API（record → 同 session 记录），冷启动耗时不确定；
  // 用 toHaveCount 自动重试替代固定 sleep，避免时序不够时误判 count=0。
  const codebuddyReply = page.locator('input[placeholder="输入回复内容..."]');
  await expect(codebuddyReply).toHaveCount(1, { timeout: 15000 });
  await page.screenshot({ path: 'test-results/codebuddy-resume.png', fullPage: true });

  // 反向：atomcode 记录的帖子页不应出现回复输入框。
  await page.goto(`${BASE}/#/todos/${atomcode!.todo_id}/posts/${atomcode!.id}`);
  // 先等网络空闲确认帖子数据已加载，再用 toHaveCount(0) 断言「不渲染」，
  // 避免在加载未完成阶段（恰好 count=0）提前通过。
  await page.waitForLoadState('networkidle');
  const atomcodeReply = page.locator('input[placeholder="输入回复内容..."]');
  await expect(atomcodeReply).toHaveCount(0, { timeout: 15000 });
});
