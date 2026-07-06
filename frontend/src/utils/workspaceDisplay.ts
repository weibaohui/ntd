import { useCallback, useEffect, useState } from 'react';
import { getProjectDirectories } from '@/utils/database/todos';
import type { ProjectDirectory } from '@/types';

/**
 * 工作空间相关展示/查询工具。
 *
 * 三种语义必须分清，混用就会出现"展示给用户的是路径而不是名称"这类 bug：
 *   - `id`     : project_directories.id（数字，后端 API / 组件间 props 唯一键）
 *   - `path`   : project_directories.path（路径字符串，仅用于 cwd/worktree，后端内部消费）
 *   - `name`   : project_directories.name（人类可读名称，UI 展示用这个）
 *
 * 约定（破坏式更新）：
 * - 组件之间 props 全部传 `id`；path 不再作 props 主键，避免重复传递与不一致。
 * - UI 展示一律用 `name`；调用需要 id 的 API 直接用 id；
 *   拿到 workspace_id 但要展示时统一走 `getWorkspaceDisplayName`。
 */

/**
 * 拿到 UI 展示用的「工作空间名称」：传入 id，在目录列表中反查 name；
 * 找不到 name 时降级到 path，再降级到空串。
 * 注意：不要把返回结果当作 id 用 —— 这里只用于展示。
 */
export function getWorkspaceDisplayName(
  dirs: ProjectDirectory[] | null | undefined,
  id: number | null | undefined,
): string {
  if (id == null) return '';
  const matched = dirs?.find(d => d.id === id);
  if (matched?.name) return matched.name;
  return matched?.path ?? '';
}

/**
 * Hook：组件挂载时一次性加载 project_directories，返回 `{ dirs, byId }`。
 *
 * - `dirs`：完整列表，传给 `getWorkspaceDisplayName` / `getWorkspacePathById` 用。
 * - `byId`：id → ProjectDirectory 的 Map，热路径上 O(1) 查找，避免每个候选 todo 都做 O(n) find。
 *
 * project_directories 是低基数集合（手动维护），一次性全量加载比按需拉更简单也更稳。
 */
export function useProjectDirectories(): {
  dirs: ProjectDirectory[];
  byId: Map<number, ProjectDirectory>;
  loading: boolean;
} {
  const [dirs, setDirs] = useState<ProjectDirectory[]>([]);
  const [loading, setLoading] = useState(false);
  const load = useCallback(() => {
    setLoading(true);
    getProjectDirectories()
      .then(setDirs)
      .catch(() => setDirs([]))
      .finally(() => setLoading(false));
  }, []);
  useEffect(() => { load(); }, [load]);
  const byId = new Map<number, ProjectDirectory>();
  for (const d of dirs) byId.set(d.id, d);
  return { dirs, byId, loading };
}
