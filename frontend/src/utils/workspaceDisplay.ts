import { useCallback, useEffect, useState } from 'react';
import { getProjectDirectories } from '@/utils/database/todos';
import type { ProjectDirectory } from '@/types';

/**
 * 工作空间相关展示/查询工具。
 *
 * 三种语义必须分清，混用就会出现"展示给用户的是路径而不是名称"这类 bug：
 *   - `id`     : project_directories.id（数字，后端 API 用这个过滤）
 *   - `path`   : project_directories.path（路径字符串，loop.workspace 字段存这个）
 *   - `name`   : project_directories.name（人类可读名称，UI 展示用这个）
 *
 * UI 展示一律用 `name`；调用需要 id 的 API 用 `getWorkspaceIdByPath`；
 * 拿到 workspace 字段但要展示时统一走 `getWorkspaceDisplayName`。
 */

/** 从一组目录里根据 path 找到 id；找不到返回 undefined。空/null 路径一律返回 undefined。 */
export function getWorkspaceIdByPath(
  dirs: ProjectDirectory[] | null | undefined,
  path: string | null | undefined,
): number | undefined {
  if (!dirs || !path || !path.trim()) return undefined;
  return dirs.find(d => d.path === path.trim())?.id;
}

/**
 * 拿到 UI 展示用的「工作空间名称」：优先 name，找不到则降级到 path，再降级到空串。
 * 注意：不要把返回结果当作 id 用 —— 这里只用于展示。
 */
export function getWorkspaceDisplayName(
  dirs: ProjectDirectory[] | null | undefined,
  path: string | null | undefined,
): string {
  if (!path) return '';
  const matched = dirs?.find(d => d.path === path);
  if (matched?.name) return matched.name;
  return path;
}

/**
 * Hook：组件挂载时一次性加载 project_directories，返回 `{ dirs, byPath }`。
 *
 * - `dirs`：完整列表，传给 `getWorkspaceDisplayName` / `getWorkspaceIdByPath` 用。
 * - `byPath`：path → ProjectDirectory 的 Map，热路径上 O(1) 查找，避免每个候选 todo 都做 O(n) find。
 *
 * project_directories 是低基数集合（手动维护），一次性全量加载比按需拉更简单也更稳。
 */
export function useProjectDirectories(): {
  dirs: ProjectDirectory[];
  byPath: Map<string, ProjectDirectory>;
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
  const byPath = new Map<string, ProjectDirectory>();
  for (const d of dirs) if (d.path) byPath.set(d.path, d);
  return { dirs, byPath, loading };
}
