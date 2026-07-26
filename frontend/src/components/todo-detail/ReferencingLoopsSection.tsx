// 事项详情「所属环路」区块：展示引用该事项的启用环路，点击跳环路详情。
//
// 数据按需查询（挂载时才请求 referencing-loops 接口），
// 避免主 todos 列表为它 JOIN 放大；空引用时整体不渲染。

import { useEffect, useState } from 'react';
import { useApp } from '@/hooks/useApp';
import { useViewState } from '@/hooks/useViewState';
import { getReferencingLoops } from '@/utils/database/todos';
import { ReferencingLoops } from '@/components/common/ReferencingLoops';
import type { LoopRefSummary } from '@/types';

interface ReferencingLoopsSectionProps {
  todoId: number;
}

export function ReferencingLoopsSection({ todoId }: ReferencingLoopsSectionProps) {
  const { state } = useApp();
  const { replaceUrl } = useViewState();
  // null = 尚未加载完成（不渲染，避免闪烁）；空数组 = 无引用（不渲染）
  const [loops, setLoops] = useState<LoopRefSummary[] | null>(null);

  // 切换事项或工作空间时重新查询；cancelled 防御快速切换造成的竞态：
  // 晚返回的请求发现已切换就丢弃，避免覆盖新事项的数据。
  useEffect(() => {
    const wsId = state.selectedWorkspace;
    if (wsId == null) return;
    let cancelled = false;
    getReferencingLoops(wsId, todoId)
      .then((data) => { if (!cancelled) setLoops(data); })
      // 失败静默降级为空：该区块是辅助溯源信息，不应打扰详情主流程
      .catch(() => { if (!cancelled) setLoops([]); });
    return () => { cancelled = true; };
  }, [todoId, state.selectedWorkspace]);

  if (!loops || loops.length === 0) return null;

  return (
    <div
      data-testid="todo-referencing-loops"
      style={{
        display: 'flex', alignItems: 'center', gap: 4,
        marginBottom: 12, fontSize: 12,
        color: 'var(--color-text-secondary, #475569)',
      }}
    >
      <span style={{ flexShrink: 0 }}>所属环路：</span>
      <ReferencingLoops
        loops={loops}
        onSelectLoop={(loopId) => replaceUrl('loops', { id: loopId, panel: 'detail' })}
      />
    </div>
  );
}
