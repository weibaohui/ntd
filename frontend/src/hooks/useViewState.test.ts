// useViewState 的 URL 构建逻辑单测（buildHashUrl 为纯函数，无 window 依赖）。
// 覆盖本次新增：帖子页返回来源编码（?from=task&taskId=）与 tasks 视图 ?tab= 支持。

import { describe, expect, it } from 'vitest';
import { buildHashUrl } from './useViewState';

describe('buildHashUrl 帖子页返回来源', () => {
  it('test_buildHashUrl_post_without_back_source_returns_plain_url', () => {
    // 无 postBack：事项侧进入帖子页，URL 不带返回来源 query（默认返回事项详情）
    expect(buildHashUrl('todos', { id: 7, recordId: 7 })).toBe('#/todos/7/posts/7');
  });

  it('test_buildHashUrl_post_from_task_appends_from_query', () => {
    // 从任务-讨论 tab 跳入帖子页：URL 带 ?from=task&taskId=，返回按钮据此回任务讨论 tab
    expect(buildHashUrl('todos', { id: 7, recordId: 7, postBack: 'task', postBackTaskId: 5 }))
      .toBe('#/todos/7/posts/7?from=task&taskId=5');
  });

  it('test_buildHashUrl_post_with_task_back_but_missing_task_id_omits_query', () => {
    // 来源声明了 task 但 taskId 缺失：不追加 query，避免生成残缺 URL
    expect(buildHashUrl('todos', { id: 7, recordId: 7, postBack: 'task' })).toBe('#/todos/7/posts/7');
  });

  it('test_buildHashUrl_post_with_todo_back_source_omits_query', () => {
    // postBack='todo' 显式声明事项来源：与缺省一致，不带 query
    expect(buildHashUrl('todos', { id: 7, recordId: 7, postBack: 'todo' })).toBe('#/todos/7/posts/7');
  });
});

describe('buildHashUrl tasks 视图 tab 参数', () => {
  it('test_buildHashUrl_task_with_tab_appends_tab_query', () => {
    // 帖子页返回任务讨论 tab 的落点：/#/tasks/:id?tab=discussion
    expect(buildHashUrl('tasks', { id: 5, tab: 'discussion' })).toBe('#/tasks/5?tab=discussion');
  });

  it('test_buildHashUrl_task_without_tab_stays_plain', () => {
    expect(buildHashUrl('tasks', { id: 5 })).toBe('#/tasks/5');
    expect(buildHashUrl('tasks')).toBe('#/tasks');
  });
});
