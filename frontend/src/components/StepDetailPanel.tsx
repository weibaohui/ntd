// 环节详情面板主组件：展示环节的基本信息、Prompt、验收标准、执行历史。
// 编辑功能委托给 StepEditDrawer，数据加载委托给 useStepDetail hook。
// 执行历史复用 useExecutionHistory hook，与 Todo 事项的执行记录展示逻辑一致。
// 通过拆分，主组件仅负责布局和交互协调，各子组件职责单一。

import { useState, useCallback, useMemo } from 'react';
import {
  Skeleton, Empty, Tag, Descriptions, Button, Popconfirm, App as AntApp,
} from 'antd';
import { ApartmentOutlined, ThunderboltOutlined, EditOutlined, DeleteOutlined, HistoryOutlined, ReloadOutlined } from '@ant-design/icons';
import { useStepDetail } from '@/hooks/useStepDetail';
import { StepEditDrawer } from '@/components/StepEditDrawer';
import * as dbSteps from '@/utils/database/steps';
import * as db from '@/utils/database';
import { formatRelativeTime } from '@/utils/datetime';
import type { StepSummary, ExecutionRecord } from '@/types';
import { useExecutionHistory } from '@/hooks/useExecutionHistory';
import { useApp } from '@/hooks/useApp';
import { HistoryList } from '@/components/todo-detail/HistoryList';
import { RecordDetailView } from '@/components/todo-detail/RecordDetailView';
import { groupBySession } from '@/components/todo-detail/helpers';

interface StepDetailPanelProps {
  stepId: number;
  // 保存成功后通知父组件刷新列表，保持左右栏数据同步
  onStepUpdated?: () => void;
  // 删除环节后通知父组件清除选中状态，防止 UI 仍显示已删除环节的详情面板
  onStepDeleted?: () => void;
}

// 基本信息区段：展示执行器、复用次数、来源事项、更新时间
// 独立为子组件，避免主组件过长，便于单独维护样式
function StepInfoSection({ step }: { step: StepSummary }) {
  return (
    <section style={sectionStyle}>
      <div style={{ fontSize: 14, fontWeight: 600, color: 'var(--color-text, #0f172a)', marginBottom: 12 }}>基本信息</div>
      <Descriptions column={2} size="small" bordered={false}>
        <Descriptions.Item label="执行器">
          {step.executor ? (
            <span><ThunderboltOutlined style={{ color: '#fa8c16', marginRight: 4 }} />{step.executor}</span>
          ) : (
            <span style={{ color: 'var(--color-text-tertiary, #94a3b8)' }}>未指派</span>
          )}
        </Descriptions.Item>
        <Descriptions.Item label="复用次数">
          <Tag icon={<ApartmentOutlined />} color={step.used_by_loop_step_count > 0 ? 'purple' : 'default'}>
            {step.used_by_loop_step_count}
          </Tag>
        </Descriptions.Item>
        <Descriptions.Item label="来源事项">
          {step.source_todo_id ? (
            <span>#<code>{step.source_todo_id}</code></span>
          ) : (
            <span style={{ color: 'var(--color-text-tertiary, #94a3b8)' }}>—</span>
          )}
        </Descriptions.Item>
        <Descriptions.Item label="更新于">
          {step.updated_at ? formatRelativeTime(step.updated_at) : '—'}
        </Descriptions.Item>
      </Descriptions>
    </section>
  );
}

// 通用文本展示区段：用于 Prompt 和验收标准的展示，
// 接受标题和内容，统一处理空值显示，减少重复代码
function TextDisplaySection({ title, content }: { title: string; content: string | null | undefined }) {
  return (
    <section style={sectionStyle}>
      <div style={{ fontSize: 14, fontWeight: 600, color: 'var(--color-text, #0f172a)', marginBottom: 8 }}>{title}</div>
      <div style={textDisplayStyle}>
        {content || <span style={{ color: 'var(--color-text-tertiary, #94a3b8)' }}>无{title}</span>}
      </div>
    </section>
  );
}

// 区段容器样式：统一的背景、边框、圆角、内边距，
// 提取为常量避免每次渲染创建新对象引用，提升性能
const sectionStyle: React.CSSProperties = {
  background: 'var(--color-bg-elevated, #ffffff)',
  border: '1px solid var(--color-border, #e2e8f0)',
  borderRadius: 8,
  padding: 16,
  marginBottom: 12,
};

// Prompt 内容样式：预格式化文本、浅色背景、适中行高，
// 与验收标准区段共用基础样式，仅追加 minHeight
const textDisplayStyle: React.CSSProperties = {
  fontSize: 13, color: 'var(--color-text-secondary, #475569)',
  background: 'var(--color-bg-secondary, #f8fafc)',
  padding: 12, borderRadius: 6, whiteSpace: 'pre-wrap',
  lineHeight: 1.6, minHeight: 40,
};

export function StepDetailPanel({ stepId, onStepUpdated, onStepDeleted }: StepDetailPanelProps) {
  const { message } = AntApp.useApp();
  const { state, dispatch } = useApp();
  const { step, loading, error, loadStep } = useStepDetail(stepId);
  const [editing, setEditing] = useState(false);
  const [viewMode, setViewMode] = useState<'log' | 'chat' | 'command'>('log');

  // 执行历史：通过 step_id 获取环节自己的执行记录
  const stepIdForHistory = step?.id ?? null;
  const {
    records,
    selectedHistoryRecordId,
    setSelectedHistoryRecordId,
    selectedHistoryRecord,
    isLoadingDetail,
    historyPage,
    historyTotal,
    historyLimit,
    loadExecutionRecords,
    handleHistoryPageChange,
    loadLogs,
    refreshSingleRecord,
    paginatedLogs,
    logsTotal,
    logsPage,
    logsPerPage,
    isLoadingLogs,
  } = useExecutionHistory({
    selectedTodoId: null,
    stepId: stepIdForHistory,
    storeRecords: [],
    dispatch,
  });

  const sessionGroups = useMemo(() => groupBySession(records), [records]);

  const handleStopExecution = async (recordId: number) => {
    try {
      await db.stopExecution(recordId);
      message.info('已发送停止指令');
      await loadExecutionRecords(historyPage, historyLimit);
    } catch (error) {
      message.error('停止失败: ' + (error instanceof Error ? error.message : String(error)));
    }
  };

  const handleRateExecution = async (recordId: number, rating: number | null) => {
    await db.rateExecutionRecord(recordId, rating);
    await refreshSingleRecord(recordId);
    message.success(rating == null ? '已清除评分' : `已评分 ${rating}`);
  };

  const handleOpenResume = (_record: ExecutionRecord) => {
    message.info('继续对话功能暂未在环节中支持');
  };

  const handleExportMarkdown = async (_record: ExecutionRecord) => {
    message.info('导出功能暂未在环节中支持');
  };

  const getRunningTaskForRecord = (record: ExecutionRecord) => {
    if (record.task_id && state.runningTasks[record.task_id]) {
      return state.runningTasks[record.task_id] as any || null;
    }
    return Object.values(state.runningTasks).find((t: any) => t.todoId === record.todo_id) || null;
  };

  const resolveExecutionStats = (record: ExecutionRecord, isRunning: boolean) => {
    if (isRunning) {
      const task = getRunningTaskForRecord(record);
      if (task?.executionStats) return task.executionStats;
    }
    return record.execution_stats;
  };

  // 删除环节：调用 API 后通知父组件清除选中状态，
  // 先通知 onStepDeleted 再通知 onStepUpdated，避免 UI 闪现
  const handleDelete = useCallback(async () => {
    if (!step) return;
    try {
      await dbSteps.deleteStep(step.id);
      message.success('环节已删除');
      onStepDeleted?.();
      onStepUpdated?.();
    } catch {
      message.error('删除失败，环节可能正在被 loop 引用');
    }
  }, [step, message, onStepUpdated, onStepDeleted]);

  // 加载态和错误态：优先判断，避免后续渲染逻辑执行
  if (loading) return <Skeleton active style={{ padding: 24 }} />;
  if (!step) return <Empty description={error || '无法加载该环节'} style={{ marginTop: 64 }} />;

  return (
    <>
      <div style={{ padding: '20px 24px' }}>
        <PanelHeader step={step} onEdit={() => setEditing(true)} onDelete={handleDelete} />
        <StepInfoSection step={step} />
        <TextDisplaySection title="提示词 (Prompt)" content={step.prompt} />
        <TextDisplaySection title="验收标准" content={step.acceptance_criteria} />

        {/* 执行历史 */}
        <div style={{ marginTop: 24 }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 12 }}>
            <HistoryOutlined />
            <span style={{ fontSize: 14, fontWeight: 600, color: 'var(--color-text, #0f172a)' }}>
              执行历史
            </span>
            {records.length > 0 && (
              <span style={{ fontSize: 12, color: 'var(--color-text-tertiary, #94a3b8)', marginLeft: 'auto' }}>
                共 {historyTotal} 条
              </span>
            )}
          </div>
          {stepIdForHistory ? (
            records.length === 0 ? (
              <Empty description="暂无执行记录" image={Empty.PRESENTED_IMAGE_SIMPLE} />
            ) : (
              <>
                <div style={{ marginBottom: 8 }}>
                  <Button
                    size="small"
                    icon={<ReloadOutlined />}
                    onClick={() => loadExecutionRecords(historyPage, historyLimit)}
                  >
                    刷新
                  </Button>
                </div>
                <div style={{ display: 'flex', gap: 12, minHeight: 300 }}>
                  <HistoryList
                    sessionGroups={sessionGroups}
                    selectedHistoryRecordId={selectedHistoryRecordId}
                    onSelectRecord={(id) => setSelectedHistoryRecordId(id)}
                    historyTotal={historyTotal}
                    historyLimit={historyLimit}
                    historyPage={historyPage}
                    onPageChange={handleHistoryPageChange}
                    onOpenResume={handleOpenResume}
                    onExportMarkdown={handleExportMarkdown}
                  />
                  {selectedHistoryRecord && (
                    <div style={{ flex: 1, minWidth: 0, borderLeft: '1px solid var(--color-border-light)', paddingLeft: 12 }}>
                      <RecordDetailView
                        isLoadingDetail={isLoadingDetail}
                        record={selectedHistoryRecord}
                        sessionGroups={sessionGroups}
                        onSelectRecord={(id) => setSelectedHistoryRecordId(id)}
                        viewMode={viewMode}
                        onViewModeChange={setViewMode}
                        onOpenResume={handleOpenResume}
                        onExportMarkdown={handleExportMarkdown}
                        onStop={handleStopExecution}
                        onRefreshSingle={refreshSingleRecord}
                        onRate={handleRateExecution}
                        paginatedLogs={paginatedLogs}
                        logsTotal={logsTotal}
                        logsPage={logsPage}
                        logsPerPage={logsPerPage}
                        onLoadLogs={loadLogs}
                        isLoadingLogs={isLoadingLogs}
                        getRunningTaskForRecord={getRunningTaskForRecord}
                        resolveExecutionStats={resolveExecutionStats}
                      />
                    </div>
                  )}
                </div>
              </>
            )
          ) : (
              <Empty description="暂无执行记录" image={Empty.PRESENTED_IMAGE_SIMPLE} />
          )}
        </div>
      </div>
      <StepEditDrawer
        open={editing}
        step={step}
        onClose={() => setEditing(false)}
        onSaved={() => { loadStep(); onStepUpdated?.(); }}
      />
    </>
  );
}

// 面板头部：标题、ID、编辑和删除按钮
function PanelHeader({ step, onEdit, onDelete }: { step: StepSummary; onEdit: () => void; onDelete: () => void }) {
  return (
    <div style={{ display: 'flex', alignItems: 'center', gap: 12, marginBottom: 20 }}>
      <h2 style={headerTitleStyle}>{step.title}</h2>
      <span style={{ color: 'var(--color-text-tertiary, #94a3b8)', fontSize: 12, fontFamily: 'monospace' }}>#{step.id}</span>
      <Button size="small" icon={<EditOutlined />} onClick={onEdit}>编辑</Button>
      <Popconfirm title="删除环节" description="删除后无法恢复" okType="danger" onConfirm={onDelete}>
        <Button size="small" danger icon={<DeleteOutlined />} />
      </Popconfirm>
    </div>
  );
}

// 标题样式：flex: 1 占满剩余空间，超长文本省略号，
// 提取为常量避免每次渲染创建新对象引用
const headerTitleStyle: React.CSSProperties = {
  margin: 0, fontSize: 18, flex: 1, minWidth: 0,
  overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap',
  color: 'var(--color-text, #0f172a)',
};
