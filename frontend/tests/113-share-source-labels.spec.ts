// 113 分享事项统一：事项中心来源标签渲染中文（专家贡献/工艺分享/技能分享/模板分享）。
// 用 route mock 固定中心接口返回 4 类分享 action 事项，不依赖开发库数据，可稳定回归。
import { test, expect } from '@playwright/test';

test('事项中心分享类来源标签为中文', async ({ page }) => {
  const mkItem = (id: number, actionType: string | null, title: string) => ({
    id, title, status: 'completed', computed_bucket: 'manual',
    action_type: actionType, workspace_id: 1,
    used_by_loop_step_count: 0, created_at: '2026-08-01T00:00:00.000Z', updated_at: '2026-08-01T00:00:00.000Z',
  });
  await page.route('**/todos/center*', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        code: 0,
        data: {
          items: [
            mkItem(1, 'expert_contribute', '分享专家测试'),
            mkItem(2, 'process_contribute', '分享工艺测试'),
            mkItem(3, 'skill_contribute', '分享技能测试'),
            mkItem(4, 'todo_contribute', '分享模板测试'),
            mkItem(5, null, '普通事项'),
          ],
          total: 5, page: 1, page_size: 200,
          bucket_counts: { manual: 5 },
          action_types: ['expert_contribute', 'process_contribute', 'skill_contribute', 'todo_contribute'],
        },
      }),
    }),
  );

  await page.goto('/#/todos');
  // 轮询等第一个中文标签出现，替代固定等待（加载完成前断言会误报）
  const expertTag = page.locator('.todo-center-card-tags .ant-tag', { hasText: '专家贡献' }).first();
  await expect(expertTag).toBeVisible({ timeout: 10000 });

  const texts = await page.locator('.todo-center-card-tags .ant-tag').allTextContents();
  const joined = texts.join('|');
  console.log('卡片标签:', JSON.stringify(texts));
  // 4 类分享 action_type 均应映射为中文标签（113 统一后事项中心只显示这几个分享事项）
  expect(joined).toContain('专家贡献');
  expect(joined).toContain('工艺分享');
  expect(joined).toContain('技能分享');
  expect(joined).toContain('模板分享');
  // 不得回退显示原始英文 action_type
  expect(joined).not.toContain('expert_contribute');
  expect(joined).not.toContain('process_contribute');
  expect(joined).not.toContain('skill_contribute');
  expect(joined).not.toContain('todo_contribute');
});
