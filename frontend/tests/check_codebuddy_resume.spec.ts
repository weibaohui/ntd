// Issue 058：验证 CodeBuddy 执行记录出现「继续对话」回复输入框。
//
// 验证逻辑：
// - todo 8 下预置两条执行记录（均 success + 带 session_id）：
//   - id=40 codebuddy → 应渲染 ReplyRow（回复输入框）
//   - id=41 atomcode（不支持 resume 的对照组）→ 不应渲染
// - ReplyRow 的判显条件是 supportsResume(record)：status 非 running +
//   有 session_id + executor 在 RESUMABLE_EXECUTORS 集合内。
//
// 运行前提：make dev 已启动（http://localhost:18088），且 dev DB 已插入上述记录。
import { test, expect } from '@playwright/test';

test('codebuddy 执行记录显示继续对话输入框，atomcode 不显示', async ({ page }) => {
  // ReplyRow 渲染在帖子页（/#/todos/:id/posts/:rid）的 ThreadGroup 末尾记录下方。
  // todo 8 下预置两条 success + 带 session_id 的记录：
  //   id=40 codebuddy → 应渲染回复输入框（resumable）
  //   id=41 atomcode（对照组，不支持 resume）→ 不应渲染

  // 帖子页按全局选中的工作空间拉取执行记录，直接访问 URL 时默认为 #0
  // 会报「todo 不属于工作空间」。用 addInitScript 在每次页面加载前写入
  // localStorage（SPA 只在启动时读一次 selected_workspace，goto 后 evaluate 太晚）。
  await page.addInitScript(() => localStorage.setItem('selected_workspace', '1'));

  // 正向：codebuddy 记录的帖子页应出现回复输入框
  await page.goto('http://localhost:18088/#/todos/8/posts/40');
  await page.waitForTimeout(3000);
  const codebuddyReply = page.locator('input[placeholder="输入回复内容..."]');
  expect(await codebuddyReply.count()).toBe(1);
  await page.screenshot({ path: 'test-results/codebuddy-resume.png', fullPage: true });

  // 反向：atomcode 记录的帖子页不应出现回复输入框
  await page.goto('http://localhost:18088/#/todos/8/posts/41');
  await page.waitForTimeout(3000);
  const atomcodeReply = page.locator('input[placeholder="输入回复内容..."]');
  expect(await atomcodeReply.count()).toBe(0);
});
