// 任务详情面板的 Tab 子组件（概览 / 执行环节 / 执行历史 / 执行看板）。
// 从 TaskDetailPanel.tsx 拆分以控制文件大小。
//
// 概览 Tab：任务需求描述 + 环路基本信息 + 全局限制 + 最新执行进度。
// 执行环路 Tab：来源工艺面包屑 + DAG 流程图（复用 LoopFlowGraph）。
// 执行历史 Tab：复用 LoopExecutionsPanel 完整组件。

import { type ReactNode } from 'react';
import {
  Tag, Descriptions, Empty,
} from 'antd';
import {
  ExclamationCircleOutlined, CheckCircleOutlined, CloseCircleOutlined,
} from '@ant-design/icons';
import { LoopStepsPanel } from '@/components/LoopStudioStepsPanel';
import { LoopExecutionsPanel } from '@/components/loop-studio/executions';
import { getWorkspaceDisplayName, useProjectDirectories } from '@/utils/workspaceDisplay';
import type { LoopDetail } from '@/types/loop';
import type { GateDefinition } from '@/types/process';
import { complexityColor, complexityLabel, statusColor } from './constants';
import styles from './TaskDetailPanel.module.css';


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

// ====== StepInfo（步骤验收标准） ======

export interface StepInfo {
  id: number;
  name: string;
  order_index: number;
  skill_names: string[];
  expected_artifacts: Array<{ name: string; path?: string; locator?: string; type: string }>;
  gate_config: GateDefinition[];
}

/** 门禁类型 → 中文标签。 */
function gateLabel(type: string): string {
  const map: Record<string, string> = {
    artifact_present: '产物存在', ai_criteria_review: 'AI 评审',
    human_approval: '人工审批', script_check: '脚本校验',
  };
  return map[type] ?? type;
}

/** 把单条门禁的关键判定条件拼成短文本。 */
function gateDetailText(gate: GateDefinition): string {
  const parts: string[] = [];
  if (gate.type === 'ai_criteria_review' && typeof gate.min_score === 'number') {
    parts.push(`阈值 ≥ ${gate.min_score} 分`);
  }
  if (gate.type === 'ai_criteria_review' && typeof gate.timeout_secs === 'number' && gate.timeout_secs > 0) {
    parts.push(`等待 ≤ ${gate.timeout_secs}s`);
  }
  if (gate.type === 'artifact_present' && gate.artifact) parts.push(`产物 ${gate.artifact}`);
  if (gate.type === 'script_check' && gate.script) parts.push(`脚本 ${gate.script}`);
  return parts.join('；');
}

// ====== Tab 子组件 ======

/** Tab 1：概览 — 环路基本信息 + 全局限制。 */
export function OverviewTab({
  task, template, loopDetail, projectDirs,
}: {
  task: { id: number; title: string; status: string };
  template?: { display_name?: string; version?: string; complexity?: string };
  loopDetail: LoopDetail | null;
  projectDirs: ReturnType<typeof useProjectDirectories>['dirs'];
}) {
  return (
    <div className={styles.paneBody}>
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
    </div>
  );
}

/** Tab 2：执行环路 — DAG 流程图 + 步骤验收标准列表。 */
export function DAGTab({
  loopDetail, steps, onOpenTodo,
}: {
  loopDetail: LoopDetail | null;
  steps: StepInfo[];
  onOpenTodo?: (todoId: number) => void;
}) {
  if (!loopDetail || !loopDetail.steps || loopDetail.steps.length === 0) {
    return <Empty description="暂无执行环路" style={{ marginTop: 48 }} />;
  }
  return (
    <div className={styles.paneBody}>
      {/* 环路描述文字，来自工艺定义 */}
      {loopDetail.description && (
        <div style={{ color: 'var(--color-text-secondary, #475569)', fontSize: 13, marginBottom: 16 }}>
          {loopDetail.description}
        </div>
      )}
      <LoopStepsPanel steps={loopDetail.steps} onOpenTodo={onOpenTodo} />
      {/* 步骤验收标准：按 order_index 排序，展示每个环节的技能/产物/门禁 */}
      {steps.length > 0 && (
        <div style={{ marginTop: 24 }}>
          <div style={{
            fontSize: 14, fontWeight: 600,
            color: 'var(--color-text, #0f172a)', marginBottom: 12,
          }}>
            验收标准
          </div>
          <div className={styles.steps}>
            {[...steps].sort((a, b) => a.order_index - b.order_index).map((s) => (
              <div key={s.id} className={styles.stepItem}>
                <div className={styles.stepIndex}>{s.order_index + 1}</div>
                <div className={styles.stepBody}>
                  <div className={styles.stepName}>{s.name}</div>
                  {s.skill_names.length > 0 && (
                    <div className={styles.stepMetaRow}>
                      <span className={styles.stepLabel}>技能</span>
                      {s.skill_names.map((sk) => (
                        <Tag key={sk} color="purple" style={{ fontSize: 12 }}>{sk}</Tag>
                      ))}
                    </div>
                  )}
                  {s.expected_artifacts.length > 0 && (
                    <div className={styles.stepMetaRow}>
                      <span className={styles.stepLabel}>产物</span>
                      {s.expected_artifacts.map((a, i) => (
                        <Tag key={i} color="blue" style={{ fontSize: 12 }}>
                          {a.name} → {a.path ?? a.locator} ({a.type})
                        </Tag>
                      ))}
                    </div>
                  )}
                  {s.gate_config.length > 0 && (
                    <div className={styles.stepMetaRow}>
                      <span className={styles.stepLabel}>门禁</span>
                      {s.gate_config.map((g, i) => {
                        const detail = gateDetailText(g);
                        const suffix = detail ? `${gateLabel(g.type)} · ${detail}` : gateLabel(g.type);
                        return (
                          <Tag key={i} style={{ fontSize: 12 }}>{g.name} ({suffix})</Tag>
                        );
                      })}
                    </div>
                  )}
                </div>
              </div>
            ))}
          </div>
        </div>
      )}
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

