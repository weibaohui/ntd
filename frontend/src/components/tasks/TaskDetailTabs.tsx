// 任务详情面板的 Tab 子组件（概览 / 执行环节 / 执行历史 / 执行看板）。
// 从 TaskDetailPanel.tsx 拆分以控制文件大小。
//
// 概览 Tab：任务需求描述 + 环路基本信息 + 全局限制 + 最新执行进度。
// 执行环节 Tab：来源工艺面包屑 + DAG 流程图（复用 LoopFlowGraph）。
// 执行历史 Tab：复用 LoopExecutionsPanel 完整组件。
// 执行看板 Tab：取最新执行记录渲染 ProcessExecutionBoard。

import { useState, useEffect, type ReactNode } from 'react';
import {
  Tag, Typography, Spin, Progress, Descriptions, Empty,
} from 'antd';
import {
  ExclamationCircleOutlined, CheckCircleOutlined, CloseCircleOutlined,
} from '@ant-design/icons';
import { ProcessExecutionBoard } from '@/components/process/ProcessExecutionBoard';
import { LoopStepsPanel } from '@/components/LoopStudioStepsPanel';
import { LoopExecutionsPanel } from '@/components/loop-studio/executions';
import { TraceBreadcrumb } from '@/components/common/TraceBreadcrumb';
import * as dbLoops from '@/utils/database/loops';
import { getWorkspaceDisplayName, useProjectDirectories } from '@/utils/workspaceDisplay';
import type { LoopDetail } from '@/types/loop';
import { complexityColor, complexityLabel, statusColor } from './constants';
import styles from './TaskDetailPanel.module.css';

const { Text, Paragraph } = Typography;

// ====== 共享工具组件 ======

/** 详情区块统一样式容器。 */
export function DetailSection({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section style={{
      background: 'var(--color-bg-elevated, #ffffff)',
      border: '1px solid var(--color-border, #e2e8f0)',
      borderRadius: 8, padding: 16, marginBottom: 12,
    }}>
      <div style={{ fontSize: 14, fontWeight: 600, color: 'var(--color-text, #0f172a)', marginBottom: 12 }}>
        {title}
      </div>
      {children}
    </section>
  );
}

/** 基本信息字段：label + value 两行布局。 */
export function DetailField({ label, value }: { label: string; value: ReactNode }) {
  return (
    <div>
      <div style={{ fontSize: 11, color: 'var(--color-text-tertiary, #94a3b8)', marginBottom: 4 }}>{label}</div>
      <div style={{ fontSize: 13, color: 'var(--color-text, #0f172a)' }}>{value}</div>
    </div>
  );
}

/** 空值占位符。 */
function EmptyValue() {
  return <span style={{ color: 'var(--color-text-tertiary, #94a3b8)' }}>—</span>;
}

/** 执行状态 → Progress 语义色。 */
function progressStatus(status: string): 'success' | 'exception' | 'active' | 'normal' {
  if (status === 'success') return 'success';
  if (status === 'failed') return 'exception';
  if (status === 'running') return 'active';
  return 'normal';
}

// ====== ExecInfo（内部类型） ======

interface ExecInfo {
  id: number;
  status: string;
  started_at?: string;
  finished_at?: string;
  total_steps: number;
  completed_steps: number;
  failed_steps: number;
  requirement?: string;
  pending_approval_count?: number;
}

// ====== Tab 子组件 ======

/** Tab 1：概览 — 需求描述 + 环路基本信息 + 全局限制 + 最新执行进度。 */
export function OverviewTab({
  task, template, executions, loopDetail, projectDirs,
}: {
  task: { id: number; title: string; status: string; description?: string };
  template?: { display_name?: string; version?: string; complexity?: string };
  executions: ExecInfo[];
  loopDetail: LoopDetail | null;
  projectDirs: ReturnType<typeof useProjectDirectories>['dirs'];
}) {
  const latest = executions[0];
  const percent = latest && latest.total_steps > 0
    ? Math.round((latest.completed_steps / latest.total_steps) * 100) : 0;

  return (
    <div className={styles.paneBody}>
      {/* 需求描述 */}
      <div style={{ marginBottom: 16 }}>
        <Text strong>需求描述</Text>
        <Paragraph style={{ marginTop: 4 }} type={task.description ? undefined : 'secondary'}>
          {task.description || '暂无描述'}
        </Paragraph>
      </div>

      {/* 环路基本信息（来自 LoopDetail） */}
      {loopDetail && (
        <DetailSection title="基本信息">
          <div style={{ display: 'grid', gridTemplateColumns: 'repeat(3, 1fr)', gap: 16 }}>
            <DetailField label="关联工作空间" value={
              loopDetail.workspace_id != null ? (() => {
                const displayName = getWorkspaceDisplayName(projectDirs, loopDetail.workspace_id);
                const dir = projectDirs.find(d => d.id === loopDetail.workspace_id);
                return dir ? (
                  <div>
                    <div style={{ fontWeight: 500 }}>{displayName}</div>
                    <div style={{ fontSize: 11, color: 'var(--color-text-tertiary, #94a3b8)', marginTop: 2 }}>
                      {dir.path}
                      {dir.git_worktree_enabled && <span style={{ marginLeft: 8 }}>· Git Worktree</span>}
                      {dir.auto_cleanup && <span style={{ marginLeft: 4 }}>· 自动清理</span>}
                    </div>
                  </div>
                ) : <span>{displayName}</span>;
              })() : <EmptyValue />
            } />
            {loopDetail.pending_approval_count > 0 && (
              <DetailField label="待审批" value={
                <span style={{
                  display: 'inline-flex', alignItems: 'center', gap: 4,
                  padding: '2px 10px', borderRadius: 12,
                  background: 'var(--color-error-bg, #fef2f2)',
                  color: 'var(--color-error, #ef4444)', fontWeight: 700, fontSize: 14,
                }}>
                  <ExclamationCircleOutlined />{loopDetail.pending_approval_count} 条待审批
                </span>
              } />
            )}
          </div>
        </DetailSection>
      )}

      {/* 全局限制（来自 LoopDetail.limits_config） */}
      {loopDetail && (
        <DetailSection title="全局限制">
          <div style={{ display: 'grid', gridTemplateColumns: 'repeat(3, 1fr)', gap: 16 }}>
            <DetailField label="最大执行步数" value={
              (() => {
                try {
                  const lc = JSON.parse(loopDetail.limits_config || '{}');
                  return lc.max_step_executions != null
                    ? <span style={{ fontWeight: 500 }}>{lc.max_step_executions} 步</span>
                    : <span style={{ color: '#94a3b8' }}>未限制</span>;
                } catch { return <EmptyValue />; }
              })()
            } />
            <DetailField label="最大 Token 数" value={
              (() => {
                try {
                  const lc = JSON.parse(loopDetail.limits_config || '{}');
                  return lc.max_total_tokens != null
                    ? <span style={{ fontWeight: 500 }}>{lc.max_total_tokens.toLocaleString()}</span>
                    : <span style={{ color: '#94a3b8' }}>未限制</span>;
                } catch { return <EmptyValue />; }
              })()
            } />
            <DetailField label="超限异常处理" value={
              (() => {
                const hasHandler = loopDetail.abnormal_handler_prompt != null && loopDetail.abnormal_handler_prompt !== '';
                const triggerOn = loopDetail.abnormal_handler_trigger_on
                  ? JSON.parse(loopDetail.abnormal_handler_trigger_on) : [];
                const enabled = hasHandler && Array.isArray(triggerOn)
                  && (triggerOn.includes('capped_step') || triggerOn.includes('capped_token'));
                return (
                  <span style={{
                    display: 'inline-flex', alignItems: 'center', gap: 6,
                    padding: '2px 8px', borderRadius: 12,
                    background: enabled ? '#dcfce7' : '#f1f5f9',
                    color: enabled ? '#166534' : '#64748b', fontSize: 12, fontWeight: 500,
                  }}>
                    {enabled ? <CheckCircleOutlined style={{ fontSize: 12 }} /> : <CloseCircleOutlined style={{ fontSize: 12 }} />}
                    {enabled ? '已启用' : '未启用'}
                  </span>
                );
              })()
            } />
          </div>
        </DetailSection>
      )}

      {/* 工艺模板基本信息（无环路时回退展示） */}
      {!loopDetail && (
        <Descriptions column={1} size="small" title="基本信息">
          <Descriptions.Item label="工艺模板">{template?.display_name ?? '—'}</Descriptions.Item>
          <Descriptions.Item label="版本">{template?.version ?? '—'}</Descriptions.Item>
          <Descriptions.Item label="复杂度">
            {template?.complexity
              ? <Tag color={complexityColor(template.complexity)}>{complexityLabel(template.complexity)}</Tag>
              : '—'}
          </Descriptions.Item>
          <Descriptions.Item label="状态">
            <Tag color={statusColor(task.status)}>{task.status}</Tag>
          </Descriptions.Item>
        </Descriptions>
      )}

      {/* 最新执行进度 */}
      {latest && (
        <div style={{ marginTop: 16 }}>
          <Text strong>最近一次执行 #{latest.id}</Text>
          <Progress percent={percent} status={progressStatus(latest.status)} style={{ marginTop: 8 }} />
          <Text type="secondary">
            完成 {latest.completed_steps}/{latest.total_steps}
            {latest.failed_steps > 0 && ` · 失败 ${latest.failed_steps}`}
          </Text>
        </div>
      )}
    </div>
  );
}

/** Tab 2：执行环节 — 来源工艺面包屑 + DAG 流程图。 */
export function DAGTab({
  loopDetail, onOpenProcess, onOpenTodo,
}: {
  loopDetail: LoopDetail | null;
  onOpenProcess?: (templateName: string) => void;
  onOpenTodo?: (todoId: number) => void;
}) {
  if (!loopDetail || !loopDetail.steps || loopDetail.steps.length === 0) {
    return <Empty description="暂无执行环节" style={{ marginTop: 48 }} />;
  }
  return (
    <div className={styles.paneBody}>
      {loopDetail.process_template_id != null && loopDetail.process_template_name && (
        <div data-testid="task-loop-source-process">
          <TraceBreadcrumb
            title="来源工艺"
            segments={[{
              label: loopDetail.process_template_display_name || loopDetail.process_template_name,
              techName: loopDetail.process_template_name,
              version: loopDetail.process_template_version || undefined,
              onClick: onOpenProcess
                ? () => onOpenProcess(loopDetail.process_template_guid ?? loopDetail.process_template_name!)
                : undefined,
            }]}
          />
        </div>
      )}
      <LoopStepsPanel steps={loopDetail.steps} onOpenTodo={onOpenTodo} />
    </div>
  );
}

/** Tab 3：执行历史 — 复用 LoopExecutionsPanel 完整组件。 */
export function ExecHistoryTab({
  loopId, workspaceId, loopName,
}: {
  loopId: number;
  workspaceId: number | null;
  loopName: string;
}) {
  if (!loopId) {
    return <Empty description="暂无关联环路，无法查看执行历史" style={{ marginTop: 48 }} />;
  }
  return (
    <div className={styles.paneBody}>
      <LoopExecutionsPanel loopId={loopId} workspaceId={workspaceId} loopName={loopName} />
    </div>
  );
}

/** Tab 4：执行看板 — 取最新执行记录渲染 ProcessExecutionBoard。 */
export function ExecBoardTab({ workspaceId, loopId }: { workspaceId: number | null; loopId: number }) {
  const [execId, setExecId] = useState<number | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    if (!workspaceId || !loopId) return;
    let cancelled = false;
    dbLoops.listExecutions(workspaceId, loopId, { limit: 1 })
      .then(res => {
        if (!cancelled && res.items.length > 0) setExecId(res.items[0].id);
      })
      .finally(() => { if (!cancelled) setLoading(false); });
    return () => { cancelled = true; };
  }, [workspaceId, loopId]);

  if (loading) return <Spin style={{ display: 'block', margin: '40px auto' }} />;
  if (execId == null || !workspaceId) {
    return <Empty description="暂无执行记录" style={{ marginTop: 48 }} />;
  }
  return (
    <div className={styles.paneBody}>
      <ProcessExecutionBoard workspaceId={workspaceId} loopId={loopId} executionId={execId} />
    </div>
  );
}
