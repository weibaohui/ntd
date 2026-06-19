// 环节数据加载 hook：封装 step 的查询、loading、error 状态，
// 让 StepDetailPanel 主组件只关注渲染和交互逻辑。

import { useEffect, useState, useCallback } from 'react';
import * as dbSteps from '@/utils/database/steps';
import type { StepSummary } from '@/types';

interface StepDetailState {
  step: StepSummary | null;
  loading: boolean;
  error: string | null;
}

// 根据 stepId 加载环节详情，返回当前状态和刷新函数，
// 刷新函数供外部保存/删除操作后调用，保持数据同步。
export function useStepDetail(stepId: number) {
  const [state, setState] = useState<StepDetailState>({
    step: null,
    loading: true,
    error: null,
  });

  const loadStep = useCallback(() => {
    setState({ step: null, loading: true, error: null });
    dbSteps.getStep(stepId)
      .then((step) => setState({ step, loading: false, error: null }))
      .catch(() => setState({ step: null, loading: false, error: '加载环节失败' }));
  }, [stepId]);

  // stepId 变化时自动重新加载
  useEffect(() => { loadStep(); }, [loadStep]);

  return { ...state, loadStep };
}
