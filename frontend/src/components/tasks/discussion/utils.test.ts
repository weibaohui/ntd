// 讨论区纯函数单元测试（vitest）。命名遵循 test_<被测函数>_<场景>（CLAUDE.md）。

import { describe, it, expect } from 'vitest';
import { detectAtToken, buildCandidates, mergeAppended, removePost } from './utils';
import type { TaskPost } from '@/types';
import type { ExpertMetadata } from '@/types/expert';

/** 构造最小 TaskPost（其余字段给默认值，按需用 over 覆盖）。 */
function post(id: number, over: Partial<TaskPost> = {}): TaskPost {
  return {
    id,
    task_id: 1,
    parent_post_id: null,
    kind: 'human',
    author_name: 'x',
    executor: null,
    expert_name: null,
    content: '',
    mentions: '[]',
    status: 'sent',
    source_execution_id: null,
    source_todo_id: null,
    created_at: null,
    updated_at: null,
    ...over,
  };
}

/** 构造最小 ExpertMetadata（仅 name/display_name_zh 影响 buildCandidates）。
 *  其余字段对本测试无意义，用 as 绕过繁琐的必填构造。 */
function expert(name: string, displayZh?: string): ExpertMetadata {
  return {
    name,
    expert_type: 'agent',
    version: '1',
    source: 'system',
    display_name_zh: displayZh ?? name,
  } as ExpertMetadata;
}

describe('detectAtToken', () => {
  it('test_detectAtToken_trailing_token_returns_query', () => {
    expect(detectAtToken('你好 @cod')).toEqual({ query: 'cod' });
  });
  it('test_detectAtToken_only_at_returns_empty_query', () => {
    expect(detectAtToken('你好 @')).toEqual({ query: '' });
  });
  it('test_detectAtToken_no_at_returns_null', () => {
    expect(detectAtToken('你好世界')).toBeNull();
  });
  it('test_detectAtToken_at_not_at_end_returns_null', () => {
    expect(detectAtToken('@cod 你好')).toBeNull();
  });
});

describe('buildCandidates', () => {
  it('test_buildCandidates_experts_before_executors', () => {
    const cs = buildCandidates('', [expert('架构师')]);
    expect(cs.length).toBeGreaterThan(0);
    // 专家组在前：首个候选 kind 应为 expert（传入的专家）。
    expect(cs[0].kind).toBe('expert');
    // 执行器组在后：存在 executor 候选（内置执行器非空）。
    expect(cs.some((c) => c.kind === 'executor')).toBe(true);
  });
  it('test_buildCandidates_filters_by_query', () => {
    const cs = buildCandidates('不存在的名字zzz', [expert('架构师')]);
    // 专家不命中；执行器也大概率不命中 → 空或极少。
    expect(cs.every((c) => !c.name.includes('架构师'))).toBe(true);
  });
  it('test_buildCandidates_caps_per_group_at_four', () => {
    const many = Array.from({ length: 10 }, (_, i) => expert(`e${i}`));
    const cs = buildCandidates('', many);
    const expertCount = cs.filter((c) => c.kind === 'expert').length;
    expect(expertCount).toBeLessThanOrEqual(4);
  });
});

describe('mergeAppended', () => {
  it('test_mergeAppended_appends_main_to_end', () => {
    const res = mergeAppended([post(1)], [post(2)]);
    expect(res.map((p) => p.id)).toEqual([1, 2]);
  });
  it('test_mergeAppended_dedups_by_id', () => {
    const res = mergeAppended([post(1)], [post(1)]);
    expect(res.map((p) => p.id)).toEqual([1]);
  });
  it('test_mergeAppended_attaches_reply_to_main', () => {
    const main = post(1);
    const reply = post(10, { parent_post_id: 1 });
    const res = mergeAppended([main], [reply]);
    expect(res[0].replies?.map((r) => r.id)).toEqual([10]);
  });
  it('test_mergeAppended_dedups_reply', () => {
    const main = { ...post(1), replies: [post(10, { parent_post_id: 1 })] };
    const res = mergeAppended([main], [post(10, { parent_post_id: 1 })]);
    expect(res[0].replies?.map((r) => r.id)).toEqual([10]);
  });
});

describe('removePost', () => {
  it('test_removePost_removes_main', () => {
    const res = removePost([post(1), post(2)], 1);
    expect(res.map((p) => p.id)).toEqual([2]);
  });
  it('test_removePost_removes_reply_in_main', () => {
    const main = { ...post(1), replies: [post(10, { parent_post_id: 1 }), post(11, { parent_post_id: 1 })] };
    const res = removePost([main], 10);
    expect(res[0].replies?.map((r) => r.id)).toEqual([11]);
  });
});
