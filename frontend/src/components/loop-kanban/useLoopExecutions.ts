// 自定义 Hook：加载并聚合所有环路的执行历史。

import { useState, useEffect } from 'react';
import * as dbLoops from '@/utils/database/loops';
import type { LoopListItem, LoopExecutionDto } from '@/types/loop';

// 环路执行记录增强类型：增加 loop_name 方便在卡片中直接显示环路名称
export interface LoopExecutionWithLoopName extends LoopExecutionDto {
  loop_name: string;
}

/**
 * 设计理由：
 * - 数据获取逻辑独立成 hook，方便测试与复用
 * - 使用 cancelledRef 防御快速切换时的竞态条件：晚返回的请求若发现已卸载，直接丢弃结果
 * - limit=20 的边界：看板场景下只需展示近期执行，20 条足够覆盖常见时间窗口且避免首屏过慢
 * - loading 状态在空列表时也能正确重置：确保空状态能正常展示，而非永久 loading
 */
export function useLoopExecutions(workspaceId?: number | null, hours?: number) {
  const [allLoops, setAllLoops] = useState<LoopListItem[]>([]);
  const [executions, setExecutions] = useState<LoopExecutionWithLoopName[]>([]);
  const [loading, setLoading] = useState(true);

  // 加载环路列表：按 workspace_id 过滤（如果传了）。
  // 切换 workspace 时先清空旧列表和执行记录，避免旧数据闪烁或触发无效的追加请求。
  useEffect(() => {
    let ignore = false;
    setAllLoops([]);
    setExecutions([]);
    setLoading(true);
    dbLoops.listLoops(workspaceId ?? undefined)
      .then(data => { if (!ignore) setAllLoops(data); })
      .catch(() => { if (!ignore) setAllLoops([]); })
      .finally(() => { if (!ignore) setLoading(false); });
    return () => { ignore = true; };
  }, [workspaceId]);

  // 环路列表加载后，批量并发拉取每个环路的执行历史。
  useEffect(() => {
    if (allLoops.length === 0) {
      setExecutions([]);
      return;
    }
    let cancelled = false;
    setLoading(true);

    Promise.all(
      allLoops.map(loop =>
        dbLoops.listExecutions(loop.id, { page: 1, limit: 20, hours: hours ?? undefined })
          .then(res => res.items.map(e => ({ ...e, loop_name: loop.name })))
          .catch(() => [])
      )
    )
      .then(results => {
        if (cancelled) return;
        const flat = results.flat();
        flat.sort((a, b) => new Date(b.started_at).getTime() - new Date(a.started_at).getTime());
        setExecutions(flat);
      })
      .catch(() => {
        if (!cancelled) setExecutions([]);
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });

    return () => { cancelled = true; };
  }, [allLoops, hours]);

  return { executions, loading };
}
