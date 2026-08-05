// 060-任务讨论区 Playwright 验证（需求 060）。
// 验证点：
// 1. 任务详情页出现「讨论」Tab；
// 2. 切到讨论 Tab 后输入器（@执行器 选择 + 发送）渲染；
// 3. 帖子流能渲染已存在的帖子（依赖后端已有数据，无则跳过该断言）。
//
// 运行前提：make dev 已起（18088）。@触发→执行→回写的完整链路由后端单测与 API 冒烟覆盖，
// 此 spec 聚焦 UI 渲染与 Tab 交互。

import { test, expect } from '@playwright/test';

const BASE = 'http://localhost:18088';

test('任务详情页讨论 Tab 渲染与输入器', async ({ page }) => {
  // 用独立详情路由（#/tasks/:id → TaskDetailPage → TaskDetailPanel，含讨论 Tab）。
  await page.goto(`${BASE}/#/tasks/39`);
  // 等待任务详情 Tab 栏渲染出现。
  await page.waitForSelector('.ant-tabs-tab', { timeout: 15000 });

  // —— 1. 出现「讨论」Tab ——
  const discussionTab = page.locator('.ant-tabs-tab', { hasText: '讨论' }).first();
  await expect(discussionTab).toBeVisible();
  // 切到讨论 Tab。
  await discussionTab.click();

  // —— 2. 输入器渲染：发送按钮（DiscussionTab 成功挂载的标志）+ Markdown 编辑器。
  //   「发送」按钮可见即说明 DiscussionComposer 已挂载；不再断言 Select 占位的具体 class
  //   （antd 版本间占位元素结构不一，过度耦合反而脆弱）。
  const sendBtn = page.getByRole('button', { name: '发送' });
  await expect(sendBtn).toBeVisible({ timeout: 10000 });
  // Markdown 编辑器容器（@uiw/react-md-editor）应出现。
  await expect(page.locator('.w-md-editor').first()).toBeVisible();

  // —— 3. 帖子流渲染：切到讨论 Tab 后应有帖子卡片（后端已有造数）或空态。
  // 二者都没有才算异常。失败时截图便于排查。
  const cardOrEmpty = await page.locator('.ant-card, .ant-empty').first().count();
  if (cardOrEmpty === 0) {
    await page.screenshot({ path: 'test-results/060-discussion-no-content.png' });
  }
  expect(cardOrEmpty).toBeGreaterThan(0);
});
