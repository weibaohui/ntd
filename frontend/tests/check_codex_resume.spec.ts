// Issue 059：验证 Codex 执行记录出现「继续对话」回复输入框。
//
// 验证逻辑：
// - todo 8 下预置 codex 执行记录（success + 带 thread_id 形式的 session_id）：
//   应渲染 ReplyRow（回复输入框）。
// - ReplyRow 的判显条件是 supportsResume(record)：status 非 running +
//   有 session_id + executor 在 RESUMABLE_EXECUTORS 集合内（codex 已于 059 加入）。
// - 复用 058 的 atomcode 记录作为对照组（如仍存在），不出现回复框。
//
// 运行前提：make dev 已启动（http://localhost:18088），且 dev DB 已插入上述记录。
import { test, expect } from '@playwright/test';

test('codex 执行记录显示继续对话输入框', async ({ page }) => {
  // 帖子页按全局选中的工作空间拉取执行记录，直接访问 URL 时默认为 #0
  // 会报「todo 不属于工作空间」。用 addInitScript 在每次页面加载前写入
  // localStorage（SPA 只在启动时读一次 selected_workspace，goto 后 evaluate 太晚）。
  await page.addInitScript(() => localStorage.setItem('selected_workspace', '1'));

  // codex 记录（id=44）的帖子页应出现回复输入框
  await page.goto('http://localhost:18088/#/todos/8/posts/44');
  await page.waitForTimeout(3000);
  const codexReply = page.locator('input[placeholder="输入回复内容..."]');
  expect(await codexReply.count()).toBe(1);
  await page.screenshot({ path: 'test-results/codex-resume.png', fullPage: true });
});
