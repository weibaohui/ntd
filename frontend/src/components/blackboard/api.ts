/**
 * 黑板域数据拉取函数（096-W4-4：从 BlackboardPage 抽离为共享 API 层）。
 *
 * 顺带收敛：`fetchWikiFileContent` 原在 BlackboardPage 与 WikiViewPage 各存一份
 * 逐字相同的定义，此处为唯一事实源。
 *
 * 均使用原生 fetch 并手动写 v1 路径（不经 axios 实例，与黑板域既有约定一致）。
 */

import type { BlackboardData, WikiFileContent, WikiFileItem } from './types';

/** 拉取黑板配置数据（设置弹窗与队列查看的数据源） */
export async function fetchBlackboardData(workspaceId: number): Promise<BlackboardData> {
  const res = await fetch(`/api/v1/workspaces/${workspaceId}/blackboard`);
  if (!res.ok) {
    throw new Error(`HTTP ${res.status}`);
  }
  const json = (await res.json()) as { data?: BlackboardData };
  if (!json.data) {
    throw new Error('Empty response body');
  }
  return json.data;
}

/** 拉取单个 Wiki 文件内容 */
export async function fetchWikiFileContent(workspaceId: number, slug: string): Promise<WikiFileContent> {
  const res = await fetch(`/api/v1/workspaces/${workspaceId}/wiki/files/${encodeURIComponent(slug)}`);
  if (!res.ok) {
    throw new Error(`HTTP ${res.status}`);
  }
  const json = (await res.json()) as { data?: WikiFileContent };
  if (!json.data) {
    throw new Error('Empty response body');
  }
  return json.data;
}

/** 拉取 Wiki 文件列表 */
export async function fetchWikiFiles(workspaceId: number): Promise<WikiFileItem[]> {
  const res = await fetch(`/api/v1/workspaces/${workspaceId}/wiki/files`);
  if (!res.ok) {
    throw new Error(`HTTP ${res.status}`);
  }
  const json = (await res.json()) as { data?: WikiFileItem[] };
  return json.data ?? [];
}
