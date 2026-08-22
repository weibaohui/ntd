import { useEffect, useState, useMemo, useCallback, useRef } from 'react';
// 093 批次2：合并版 useApp 拆为细粒度订阅——todo 域（selectedWorkspace/selectedTodoId）
// 与 exec 域（executionRecords/runningTasks）分开，uiState 变化不再触发本组件重渲染。
import { useTodos } from '@/hooks/useTodoContext';
import { useExecution } from '@/hooks/useExecutionContext';
import { useIsMobile } from '@/hooks/useIsMobile';
import { useExecutionHistory } from '@/hooks/useExecutionHistory';
import { Button, Empty, App, Modal, Input, Skeleton } from 'antd';
import { CheckCircleOutlined, ReloadOutlined, ThunderboltOutlined } from '@ant-design/icons';
import { TodoDrawer } from './TodoDrawer';
import { BREAKPOINTS, TODO_LIST_REFRESH_EVENT } from '@/constants';
import * as db from '@/utils/database';
import { extractTitle } from '@/utils/titleExtractor';
import type { ExecutionRecord, Todo } from '@/types';
import { groupBySession } from './todo-detail/helpers';
import { DetailHeader } from './todo-detail/DetailHeader';
import { ForumPostList } from './todo-detail/ForumPostList';
import { ReferencingLoopsSection } from './todo-detail/ReferencingLoopsSection';
import type { TodoDetailActionsProps } from './todo-detail/TodoDetailActions';

interface TodoDetailProps {
  hideTitleRow?: boolean;
  onOpenPost?: (todoId: number, recordId: number) => void;
  /** 独立路由场景：把操作按钮上下文上报给外层 PageCard 的 extra 区。
   *  hideTitleRow=true 时内层标题行（含按钮）整体隐藏，外层通过此回调拿到按钮上下文
   *  在 PageCard 头部右上角渲染优化标题/编辑/删除，避免按钮连带消失；
   *  返回按钮由 PageCard 按 062 约定统一渲染在 extra 最右端，不占用本上下文。 */
  onActionsReady?: (ctx: TodoDetailActionsProps | null) => void;
}

/**
 * 按 id 加载 todo：每次都发请求，不读任何本地列表缓存。
 * 设计原因：用户明确要求"不要用这个大 todo 列表"，详情页数据应始终从后端拉取，
 * 保证最新且不依赖列表是否已加载。
 *
 * 请求策略：后端路径强制带 ws_id 且做归属校验，URL /#/todos/:id 不带 ws，
 * 前端只用当前 selectedWorkspace 作为查询条件。若 todo 不属于该 workspace
 * （403）或不存在（404），返回 null 触发 UI 错误态。
 * 跨 workspace 直达场景需用户先切换 workspace 再打开 todo。
 */
async function loadTodoById(
  todoId: number,
  selectedWorkspace: number | null,
): Promise<Todo | null> {
  // selectedWorkspace 未设置时无法查询（后端路径需要 ws_id）
  if (selectedWorkspace == null) return null;
  try {
    return await db.getTodo(selectedWorkspace, todoId);
  } catch {
    // 归属不匹配（403）或 todo 不存在（404），返回 null 触发 UI 错误态
    return null;
  }
}

export function TodoDetail({ hideTitleRow = false, onOpenPost, onActionsReady }: TodoDetailProps) {
  const { state: todoState, dispatch: todoDispatch } = useTodos();
  const { state: execState, dispatch: execDispatch } = useExecution();
  const { message } = App.useApp();
  const { selectedTodoId, selectedWorkspace } = todoState;
  const { executionRecords, runningTasks } = execState;
  const isWide = !useIsMobile(BREAKPOINTS.wide);

  // selectedTodo 改为独立请求获取的 local state，不再依赖 todos 列表缓存。
  // 设计原因：URL /#/todos/:id 直接访问时，selectedWorkspace 可能与目标 todo 不同桶，
  // 依赖 visibleTodos.find 会落空导致空白页。改为按 selectedTodoId 主动请求，
  // 详情页数据始终最新，且不依赖列表是否已加载。
  const [selectedTodo, setSelectedTodo] = useState<Todo | null>(null);
  // 加载态：首次进入或切换 todo 时显示骨架屏，避免空白页误导用户
  const [todoLoading, setTodoLoading] = useState(false);
  // 加载失败态：所有候选 workspace 都查不到时显示错误提示 + 重试按钮
  const [todoLoadError, setTodoLoadError] = useState(false);

  // 竞态保护：每次发起加载请求时自增 requestId，请求 resolve 时校验是否仍为最新。
  // 场景：用户点重试后又在请求返回前切换 todo，旧 Promise 若无条件 setSelectedTodo
  // 会用旧 todo 覆盖刚加载好的新 todo 数据（TOCTOU）。用 ref 比对即可丢弃过期响应。
  const loadRequestIdRef = useRef(0);

  // 共享加载函数：集中处理 setTodoLoading / setTodoLoadError / loadTodoById 及结果更新，
  // 供初始 useEffect 和「重试」按钮共同调用，消除重复代码并统一竞态保护。
  // 拆为 fetchTodo（发起请求 + 竞态比对）与 applyTodoResult（写入 local state）两个
  // 职责单一的子函数，各自函数体不超过 30 行。
  const fetchTodo = useCallback(
    (todoId: number, ws: number | null) => {
      // 本次请求的唯一标识，resolve 时用它判断结果是否已过期
      const reqId = ++loadRequestIdRef.current;
      setTodoLoading(true);
      setTodoLoadError(false);
      loadTodoById(todoId, ws)
        .then(todo => {
          // 竞态保护：若期间又发起了新请求（reqId 已被覆盖），直接丢弃本次结果
          if (loadRequestIdRef.current !== reqId) return;
          applyTodoResult(todo);
        })
        .catch(() => {
          if (loadRequestIdRef.current !== reqId) return;
          applyTodoResult(null);
        })
        .finally(() => {
          if (loadRequestIdRef.current !== reqId) return;
          setTodoLoading(false);
        });
    },
    [],
  );

  // 写入加载结果到 local state：todo 非空则更新详情，否则触发错误态
  const applyTodoResult = useCallback((todo: Todo | null) => {
    if (todo) {
      setSelectedTodo(todo);
    } else {
      setSelectedTodo(null);
      setTodoLoadError(true);
    }
  }, []);

  // 监听 selectedTodoId / selectedWorkspace 变化，主动加载 todo 详情。
  // 加入 selectedWorkspace 依赖：冷启动直达 /#/todos/:id 时 ws 可能尚未解析（null），
  // 此时 loadTodoById 会提前返回 null 触发误报错误态；待 ws 解析完成后
  // 该 effect 会因依赖变化再次触发，用真实 ws_id 重新请求。
  useEffect(() => {
    if (selectedTodoId == null) {
      setSelectedTodo(null);
      setTodoLoadError(false);
      // 切换到无选中态时重置竞态计数，避免上一轮 pending 请求污染新状态
      loadRequestIdRef.current = 0;
      return;
    }
    // ws 尚未解析时不发请求，避免 null workspace 直接命中错误态分支；
    // 依赖里的 selectedWorkspace 变化会重新触发本 effect，自动重试。
    if (selectedWorkspace == null) return;
    fetchTodo(selectedTodoId, selectedWorkspace);
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedTodoId, selectedWorkspace]);

  // 重新加载当前 todo（供回调成功后调用，确保 local state 与后端一致）
  const reloadSelectedTodo = useCallback(async () => {
    if (selectedTodoId == null) return;
    const fresh = await loadTodoById(selectedTodoId, selectedWorkspace);
    if (fresh) setSelectedTodo(fresh);
  }, [selectedTodoId, selectedWorkspace]);

  const [todoDrawerOpen, setTodoDrawerOpen] = useState(false);

  // 使用 useExecutionHistory hook 获取执行历史相关的状态和操作
  const {
    selectedHistoryRecordId,
    setSelectedHistoryRecordId,
    records,
    historyPage,
    historyLimit,
    historyTotal,
    summary,
    selectedHistoryRecord,
    loadExecutionRecords,
    loadLogs,
    refreshSingleRecord,
    handleHistoryPageChange,
  } = useExecutionHistory({
    selectedTodoId,
    workspaceId: selectedWorkspace,
    storeRecords: selectedTodoId ? executionRecords[selectedTodoId] : [],
    // 093 批次2：hook 内部只 dispatch 执行域 action（SET/UPDATE_EXECUTION_RECORD），
    // 配合把 dispatch prop 收窄到 Dispatch<ExecutionAction>
    dispatch: execDispatch,
  });

  // Timer for live duration display of running records
  const isExecuting = Object.values(runningTasks).some(
    t => t.todoId === selectedTodoId && t.status === 'running'
  );

  const [, setTick] = useState(0);
  useEffect(() => {
    if (!isExecuting) return;
    const interval = setInterval(() => {
      setTick(t => t + 1);
    }, 1000);
    return () => clearInterval(interval);
  }, [isExecuting]);

  // 当执行结束时，刷新执行记录和日志
  const prevIsExecutingRef = useRef(isExecuting);
  useEffect(() => {
    const prev = prevIsExecutingRef.current;
    // 当 isExecuting 从 true 变为 false 时，表示执行刚结束
    if (prev && !isExecuting && selectedTodoId) {
      // 刷新执行记录列表（包含结论）
      loadExecutionRecords(historyPage, historyLimit);
      // 如果有选中的记录，刷新单条记录详情（包含 result）和日志
      if (selectedHistoryRecordId) {
        refreshSingleRecord(selectedHistoryRecordId);
        loadLogs(selectedHistoryRecordId, 1);
      }
    }
    prevIsExecutingRef.current = isExecuting;
  }, [isExecuting, selectedTodoId, selectedHistoryRecordId, historyPage, historyLimit, loadExecutionRecords, refreshSingleRecord, loadLogs]);

  const getRunningTaskForRecord = (record: ExecutionRecord) => {
    if (record.task_id) {
      return runningTasks[record.task_id] || null;
    }
    return Object.values(runningTasks).find(t => t.todoId === record.todo_id) || null;
  };

  useEffect(() => {
    if (records.length === 0) return;
    if (selectedHistoryRecordId !== null && records.find(r => r.id === selectedHistoryRecordId)) return;
    setSelectedHistoryRecordId(records[0].id);
  }, [records, selectedHistoryRecordId]);

  const handleExecute = async () => {
    if (!selectedTodo) return;
    try {
      const result = await db.executeTodo(
        selectedTodo.workspace_id!,
        selectedTodo.id,
        selectedTodo.executor || undefined,
        undefined
      );
      message.success('任务已开始执行');
      // 获取新创建的执行记录并立即添加到状态中
      try {
        const newRecord = await db.getExecutionRecord(selectedTodo.workspace_id!, result.record_id);
        execDispatch({
          type: 'ADD_EXECUTION_RECORD',
          payload: { todoId: selectedTodo.id, record: newRecord }
        });
        // 选中新记录
        setSelectedHistoryRecordId(result.record_id);
      } catch {
        // 获取新记录失败时回退到刷新列表
        await loadExecutionRecords(1, historyLimit);
      }
    } catch (error) {
      message.error('执行失败: ' + (error instanceof Error ? error.message : String(error)));
    }
  };

  const [executeWithArgsModalOpen, setExecuteWithArgsModalOpen] = useState(false);
  const [executeArgs, setExecuteArgs] = useState('');
  const [executeWithArgsLoading, setExecuteWithArgsLoading] = useState(false);

  const handleOpenExecuteWithArgs = () => {
    setExecuteArgs('');
    setExecuteWithArgsModalOpen(true);
  };

  const handleExecuteWithArgs = async () => {
    if (!selectedTodo) return;
    setExecuteWithArgsLoading(true);
    try {
      const params = executeArgs.trim() ? { message: executeArgs.trim() } : undefined;
      const result = await db.executeTodo(
        selectedTodo.workspace_id!,
        selectedTodo.id,
        selectedTodo.executor || undefined,
        params
      );
      message.success('任务已开始执行');
      setExecuteWithArgsModalOpen(false);
      setExecuteArgs('');
      // 获取新创建的执行记录并立即添加到状态中
      try {
        const newRecord = await db.getExecutionRecord(selectedTodo.workspace_id!, result.record_id);
        execDispatch({
          type: 'ADD_EXECUTION_RECORD',
          payload: { todoId: selectedTodo.id, record: newRecord }
        });
        // 选中新记录
        setSelectedHistoryRecordId(result.record_id);
      } catch {
        // 获取新记录失败时回退到刷新列表
        await loadExecutionRecords(1, historyLimit);
      }
    } catch (error) {
      message.error('执行失败: ' + (error instanceof Error ? error.message : String(error)));
    } finally {
      setExecuteWithArgsLoading(false);
    }
  };

  const sessionGroups = useMemo(() => groupBySession(records), [records]);

  const handleStatusChange = useCallback(async (newStatus: string) => {
    if (!selectedTodo) return;
    try {
      const updated = await db.updateTodo(selectedTodo.workspace_id!, selectedTodo.id, selectedTodo.title, selectedTodo.prompt || '', newStatus);
      // 056：全局桶已删除——详情页本地 state 为准，列表页经刷新事件重拉
      setSelectedTodo(updated);
      window.dispatchEvent(new Event(TODO_LIST_REFRESH_EVENT));
      message.success('状态已更新');
    } catch {
      // ignore: interceptor already shows error
    }
  }, [selectedTodo, todoDispatch]);

  const handleTitleUpdate = useCallback(async (aiResult: string) => {
    if (!selectedTodo) return;
    // 从 AI 结果中提取纯标题（处理 AI 可能返回额外解释的情况）
    const newTitle = extractTitle(aiResult);
    if (!newTitle) {
      throw new Error('无法从 AI 结果中提取标题');
    }
    const updated = await db.updateTodo(
      selectedTodo.workspace_id!,
      selectedTodo.id,
      newTitle,
      selectedTodo.prompt || '',
      selectedTodo.status || 'pending',
      selectedTodo.executor || undefined,
      selectedTodo.scheduler_enabled,
      selectedTodo.scheduler_config,
      selectedTodo.workspace_id,
      selectedTodo.webhook_enabled,
      selectedTodo.acceptance_criteria,
      selectedTodo.auto_review_enabled,
    );
    // 056：全局桶已删除——详情页本地 state 为准，列表页经刷新事件重拉
    setSelectedTodo(updated);
    window.dispatchEvent(new Event(TODO_LIST_REFRESH_EVENT));
  }, [selectedTodo, todoDispatch]);

  // 升级/降级已移除：环节与 Todo 合一，无需 promote 流程

  // useCallback 包裹：onActionsReady 上报的 ctx 引用 handleDelete，稳定化避免每次渲染都重报。
  const handleDelete = useCallback(async () => {
    if (!selectedTodo) return;
    try {
      await db.deleteTodo(selectedTodo.workspace_id!, selectedTodo.id);
      // 056：全局桶已删除——通知列表页重拉
      window.dispatchEvent(new Event(TODO_LIST_REFRESH_EVENT));
      todoDispatch({ type: 'SELECT_TODO', payload: null });
      message.success('删除成功');
    } catch {
      // ignore: interceptor already shows error
    }
  }, [selectedTodo, todoDispatch, message]);

  // 独立路由场景：把操作按钮上下文上报给外层 PageCard 的 extra 区（062 修正注释，原误写 titleSuffix）。
  // selectedTodo 为空时上报 null（加载中/错误态），外层相应不渲染按钮。
  // 依赖 handleDelete/handleTitleUpdate（均 useCallback 稳定），避免每次渲染重报。
  useEffect(() => {
    if (!onActionsReady) return;
    if (!selectedTodo) {
      onActionsReady(null);
      return;
    }
    onActionsReady({
      todo: selectedTodo,
      onDelete: handleDelete,
      onEdit: () => setTodoDrawerOpen(true),
      onTitleUpdate: handleTitleUpdate,
    });
  }, [selectedTodo, handleDelete, handleTitleUpdate, onActionsReady]);

  if (!selectedTodo) {
    // 加载中：显示骨架屏，避免空白页误导用户以为数据丢失
    if (todoLoading) {
      return (
        <div className="detail-panel" style={{ padding: 16 }}>
          <Skeleton active paragraph={{ rows: 6 }} />
        </div>
      );
    }
    // 加载失败：显示错误提示 + 重试按钮，让用户能主动恢复
    if (todoLoadError) {
      return (
        <div className="detail-panel" style={{ display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
          <div className="empty-state">
            <Empty
              description={
                <div style={{ color: 'var(--color-text-tertiary)', fontSize: 14 }}>
                  任务加载失败或不存在
                </div>
              }
              image={Empty.PRESENTED_IMAGE_SIMPLE}
            >
              <Button
                icon={<ReloadOutlined />}
                onClick={() => {
                  // 复用共享加载函数，与初始 useEffect 走同一条带竞态保护的路径，
                  // 避免重试 Promise 在用户切换 todo 后用旧结果覆盖新 todo。
                  if (selectedTodoId != null) {
                    fetchTodo(selectedTodoId, selectedWorkspace);
                  }
                }}
              >
                重试
              </Button>
            </Empty>
          </div>
        </div>
      );
    }
    // 未选择 todo（selectedTodoId 为 null）：显示空态引导
    return (
      <div className="detail-panel" style={{ display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
        <div className="empty-state">
          <div className="empty-state-icon">
            <CheckCircleOutlined />
          </div>
          <Empty
            description={
              <div style={{ color: 'var(--color-text-tertiary)', fontSize: 14 }}>
                选择一个任务查看详情
              </div>
            }
            image={Empty.PRESENTED_IMAGE_SIMPLE}
          />
        </div>
      </div>
    );
  }

  const executor = selectedTodo.executor || 'claudecode';

  const currentTodoProgress = (() => {
    const source = selectedHistoryRecord
      || (selectedHistoryRecordId ? records.find(r => r.id === selectedHistoryRecordId) : null)
      || (records.length > 0 ? records[0] : null);
    if (!source) return null;
    if (source.status === 'running') {
      const task = getRunningTaskForRecord(source);
      if (task?.todoProgress?.length) return task.todoProgress;
    }
    if (source.todo_progress) {
      try {
        const parsed = JSON.parse(source.todo_progress);
        if (Array.isArray(parsed) && parsed.length > 0) return parsed;
      } catch { /* ignore */ }
    }
    return null;
  })();

  return (
    <div className={`detail-panel${isWide ? ' detail-panel-wide' : ''}`}>
      <DetailHeader
        selectedTodo={selectedTodo}
        executor={executor}
        isExecuting={isExecuting}
        summary={summary}
        currentTodoProgress={currentTodoProgress}
        records={records}
        onDelete={handleDelete}
        onTodoDrawerOpen={() => setTodoDrawerOpen(true)}
        onOpenExecuteWithArgs={handleOpenExecuteWithArgs}
        onExecute={handleExecute}
        onStatusChange={handleStatusChange}
        onTitleUpdate={handleTitleUpdate}
        hideTitleRow={hideTitleRow}
      />

      {/* 所属环路溯源：事项被哪些启用环路引用（「事项 → 环路」向上回溯），
          无引用时区块整体不渲染，不占用详情页空间 */}
      {selectedTodoId != null && <ReferencingLoopsSection todoId={selectedTodoId} />}

      {/* Execution History */}
      <div style={{ flex: 1, display: 'flex', flexDirection: 'column', overflow: 'hidden', minHeight: 0 }}>
        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', flexShrink: 0, marginBottom: 12 }}>
          <h4 style={{ margin: 0, fontSize: 15, fontWeight: 700, color: 'var(--color-text)' }}>执行历史</h4>
          <Button
            type="text"
            size="small"
            icon={<ReloadOutlined />}
            onClick={() => loadExecutionRecords(historyPage, historyLimit)}
            loading={isExecuting}
          >
            刷新
          </Button>
        </div>
        {records.length === 0 ? (
          <Empty description="暂无执行记录" image={Empty.PRESENTED_IMAGE_SIMPLE} />
        ) : (
          <div style={{ flex: 1, display: 'flex', flexDirection: 'column', overflow: 'hidden', minHeight: 0 }}>
            <ForumPostList
              sessionGroups={sessionGroups}
              selectedRecordId={selectedHistoryRecordId}
              onSelectRecord={(id) => {
                setSelectedHistoryRecordId(id);
                if (selectedTodoId && onOpenPost) {
                  onOpenPost(selectedTodoId, id);
                }
              }}
              historyTotal={historyTotal}
              historyLimit={historyLimit}
              historyPage={historyPage}
              onPageChange={handleHistoryPageChange}
              todoTitle={selectedTodo?.title || '未命名'}
            />
          </div>
        )}
      </div>

      <TodoDrawer
        open={todoDrawerOpen}
        todo={selectedTodo}
        onClose={() => setTodoDrawerOpen(false)}
        onSaved={() => {
          // 抽屉保存可能修改多个字段（标题/prompt/执行器/调度等），
          // 直接用 reloadSelectedTodo 拉取最新数据，避免 local state 滞后。
          reloadSelectedTodo();
          // 056：全局桶已删除——通知列表页重拉当前页，保持列表与详情一致
          window.dispatchEvent(new Event(TODO_LIST_REFRESH_EVENT));
          if (selectedTodoId) {
            loadExecutionRecords(1, historyLimit);
          }
        }}
      />

      <Modal
        title={<><ThunderboltOutlined style={{ marginRight: 8 }} />带参执行</>}
        open={executeWithArgsModalOpen}
        onOk={handleExecuteWithArgs}
        onCancel={() => {
          setExecuteWithArgsModalOpen(false);
          setExecuteArgs('');
        }}
        confirmLoading={executeWithArgsLoading}
        okText="开始执行"
        cancelText="取消"
      >
        <p style={{ marginBottom: 12, color: 'var(--color-text-secondary)' }}>
          输入补充信息，将与任务原有内容一起执行：
        </p>
        <Input.TextArea
          value={executeArgs}
          onChange={(e) => setExecuteArgs(e.target.value)}
          rows={4}
          placeholder="输入补充信息..."
        />
      </Modal>
    </div>
  );
}
