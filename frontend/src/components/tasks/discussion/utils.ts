// 讨论区纯函数(从 DiscussionTab/DiscussionComposer 提取,便于单测)。
// 不含 React 副作用,只做数据变换:@检测、候选构造、列表合并/移除。

import { EXECUTORS_FOR_PICKER } from '@/utils/executors';
import { getExpertDisplayName } from '@/types/expert';
import type { ExpertMetadata } from '@/types/expert';
import type { TaskPost } from '@/types';

/** 候选项:专家或执行器,统一用 name(规范名,插入正文 + 后端匹配)。 */
export interface MentionCandidate {
  kind: 'expert' | 'executor';
  /** 规范名:专家 expert.name;执行器 executor.value。 */
  name: string;
  /** 展示名。 */
  display: string;
}

/** 候选浮层每组(专家/执行器)最多展示条数,避免列表过长。 */
const MAX_PER_GROUP = 4;

/**
 * 检测正文末尾正在输入的 @token:末尾 `@` 后跟非空白/非@ 字符。
 * 返回 query(@ 之后的文本,空串=刚输入@);无匹配返回 null。
 * 只覆盖「在末尾输入 @」的常见场景;中间插入不触发(MVP 取舍)。
 */
export function detectAtToken(value: string): { query: string } | null {
  const m = value.match(/@([^\s@]*)$/);
  return m ? { query: m[1] } : null;
}

/**
 * 按 query 过滤专家 + 执行器,构造候选列表(专家优先,与后端 resolve_mentions 消歧顺序一致)。
 * query 为空各取前 MAX_PER_GROUP;非空则按规范名/展示名包含 query 过滤。
 */
export function buildCandidates(query: string, experts: ExpertMetadata[]): MentionCandidate[] {
  const q = query.trim().toLowerCase();
  // 命中条件:无 query(刚输入@)全显;否则规范名或展示名包含 query。
  const hit = (norm: string, disp: string) => !q || norm.toLowerCase().includes(q) || disp.toLowerCase().includes(q);
  const exps: MentionCandidate[] = experts
    .filter((ex) => hit(ex.name, getExpertDisplayName(ex)))
    .slice(0, MAX_PER_GROUP)
    .map((ex) => ({ kind: 'expert', name: ex.name, display: getExpertDisplayName(ex) }));
  const execs: MentionCandidate[] = EXECUTORS_FOR_PICKER
    .filter((e) => hit(e.value, e.label))
    .slice(0, MAX_PER_GROUP)
    .map((e) => ({ kind: 'executor', name: e.value, display: e.label }));
  return [...exps, ...execs];
}

/**
 * 把刚发出的帖子并入当前列表:主楼层追加到末尾,楼中楼挂到对应主楼层 replies。
 * 按已有的全部 id(主楼层 + 各自 replies)去重——事件驱动的刷新可能在乐观并入后
 * 又把刚发的帖随列表一起拉回，不去重会产生重复帖与重复 React key。
 */
export function mergeAppended(posts: TaskPost[], appended: TaskPost[]): TaskPost[] {
  const existingIds = new Set<number>();
  posts.forEach((p) => {
    existingIds.add(p.id);
    p.replies?.forEach((r) => existingIds.add(r.id));
  });
  const fresh = appended.filter((p) => !existingIds.has(p.id));
  const mains = fresh.filter((p) => p.parent_post_id === null);
  const replies = fresh.filter((p) => p.parent_post_id !== null);
  let next = posts;
  if (replies.length) {
    // 楼中楼挂到目标主楼层(按 parent 匹配);找不到目标则丢弃(刚回复的楼层必在当前页)。
    next = next.map((p) => {
      const mine = replies.filter((r) => r.parent_post_id === p.id);
      // 只在确有挂载时产生新对象,减少无谓渲染。
      return mine.length ? { ...p, replies: [...(p.replies ?? []), ...mine] } : p;
    });
  }
  // 主楼层(含 @ 触发的 agent 占位帖)追加到末尾,契合 id ASC 的时间顺序。
  return mains.length ? [...next, ...mains] : next;
}

/** 从列表移除一条帖子:主楼层整条剔除(含其楼中楼),楼中楼则在对应楼层 replies 内过滤。 */
export function removePost(posts: TaskPost[], id: number): TaskPost[] {
  if (posts.some((p) => p.id === id)) {
    // 命中主楼层:整条移除(其 replies 随之消失,与后端 CASCADE 一致)。
    return posts.filter((p) => p.id !== id);
  }
  // 否则是楼中楼:在每条主楼层的 replies 里过滤掉它。
  return posts.map((p) => ({ ...p, replies: p.replies?.filter((r) => r.id !== id) }));
}
