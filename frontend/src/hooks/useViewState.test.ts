// useViewState 的 URL 构建逻辑单测（buildHashUrl 为纯函数，无 window 依赖）。
// 覆盖本次新增：帖子页返回来源编码（?from=task&taskId=）、tasks 视图 ?tab= 支持、
// 109 列表形态直达路由（?view= 参数）。

import { describe, expect, it } from 'vitest';
import { buildHashUrl, parsePostBackFrom, pickListView } from './useViewState';

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

describe('parsePostBackFrom 返回来源解析', () => {
  it('test_parsePostBackFrom_task_with_valid_id_returns_task', () => {
    expect(parsePostBackFrom(new URLSearchParams('from=task&taskId=5'))).toEqual({ from: 'task', taskId: 5 });
  });

  it('test_parsePostBackFrom_task_missing_id_falls_back_to_todo', () => {
    // from=task 但缺 taskId：来源无效，回退事项详情
    expect(parsePostBackFrom(new URLSearchParams('from=task'))).toEqual({ from: 'todo', taskId: null });
  });

  it('test_parsePostBackFrom_task_invalid_id_falls_back_to_todo', () => {
    // taskId 非数字（NaN）→ 回退；非正数（0）→ 回退
    expect(parsePostBackFrom(new URLSearchParams('from=task&taskId=abc'))).toEqual({ from: 'todo', taskId: null });
    expect(parsePostBackFrom(new URLSearchParams('from=task&taskId=0'))).toEqual({ from: 'todo', taskId: null });
  });

  it('test_parsePostBackFrom_without_from_returns_todo', () => {
    // 无 from 参数（或无关 query 如 ?tab=）→ 默认事项来源
    expect(parsePostBackFrom(new URLSearchParams('tab=discussion'))).toEqual({ from: 'todo', taskId: null });
  });
});

describe('buildHashUrl 109 列表形态 ?view= 参数', () => {
  it('test_buildHashUrl_todos_view_appends_query', () => {
    // 事项列表形态直达：/#/todos?view=list
    expect(buildHashUrl('todos', { view: 'list' })).toBe('#/todos?view=list');
    expect(buildHashUrl('todos', { view: 'running' })).toBe('#/todos?view=running');
  });

  it('test_buildHashUrl_todos_without_view_stays_plain', () => {
    // 无形态参数 → 不带 query（兼容旧 URL）；详情/帖子页也不携带形态参数
    expect(buildHashUrl('todos')).toBe('#/todos');
    expect(buildHashUrl('todos', { id: 7 })).toBe('#/todos/7');
    expect(buildHashUrl('todos', { id: 7, recordId: 7 })).toBe('#/todos/7/posts/7');
  });

  it('test_buildHashUrl_todos_blank_view_omits_query', () => {
    // 空白形态参数不生成残缺 query URL
    expect(buildHashUrl('todos', { view: '  ' })).toBe('#/todos');
  });

  it('test_buildHashUrl_loops_view_appends_query', () => {
    // 环路形态直达：/#/loops?view=kanban
    expect(buildHashUrl('loops', { view: 'kanban' })).toBe('#/loops?view=kanban');
    expect(buildHashUrl('loops')).toBe('#/loops');
    expect(buildHashUrl('loops', { id: 3 })).toBe('#/loops/3');
  });

  it('test_buildHashUrl_tasks_view_appends_query', () => {
    // 任务形态直达：/#/tasks?view=card；详情页只带 tab 不带 view
    expect(buildHashUrl('tasks', { view: 'card' })).toBe('#/tasks?view=card');
    expect(buildHashUrl('tasks', { id: 5, tab: 'discussion' })).toBe('#/tasks/5?tab=discussion');
  });

  it('test_buildHashUrl_view_param_is_trimmed', () => {
    // review 修复：四视图的 ?view= 写入前统一 trim（与 appendListView 口径一致），
    // 避免调用方误传 ' card ' 时 URL 携带空格、不同实例 listView 不一致
    expect(buildHashUrl('tasks', { view: ' card ' })).toBe('#/tasks?view=card');
    expect(buildHashUrl('todos', { view: ' list ' })).toBe('#/todos?view=list');
    expect(buildHashUrl('processes', { view: ' mine ' })).toBe('#/processes?view=mine');
  });

  it('test_buildHashUrl_processes_view_appends_query', () => {
    // 工艺「我的/模板」范围直达：/#/processes?view=template；
    // 编辑器态（new/edit）不携带形态参数，避免编辑器 URL 挂着无关 query
    expect(buildHashUrl('processes', { view: 'template' })).toBe('#/processes?view=template');
    expect(buildHashUrl('processes', { processMode: 'edit', guid: 'g1', view: 'template' }))
      .toBe('#/processes?guid=g1&processMode=edit');
  });
});

describe('pickListView 109 形态选择', () => {
  it('test_pickListView_valid_raw_wins', () => {
    // URL 参数合法 → 以 URL 为准（直达优先）
    expect(pickListView('list', ['list', 'card'], 'card')).toBe('list');
  });

  it('test_pickListView_invalid_raw_falls_back', () => {
    // URL 参数非法（含已废弃形态）→ 回退 localStorage 记忆
    expect(pickListView('kanban', ['list', 'card'], 'card')).toBe('card');
  });

  it('test_pickListView_null_raw_falls_back', () => {
    // URL 无形态参数 → 回退 localStorage 记忆（保持旧行为）
    expect(pickListView(null, ['list', 'card'], 'list')).toBe('list');
  });
});
