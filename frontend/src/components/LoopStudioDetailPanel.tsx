// Loop Studio 右栏 detail 容器。
//
// 对齐参考设计: 详情面板分成上下分段:
// - Header: 标题 + 操作 (触发/复制/启用暂停/编辑/删除)
// - 基本信息: 启用 Switch + 工作空间
// - 执行环节: 横向卡片列表（按顺序执行）, 最重要放在最前
// - 触发条件: 默认折叠, 仅展示已启用/共多少摘要
// - 执行历史: 折叠区, 默认收起 (不常用)
//
// (对齐 LoopDto 的可编辑字段)。

import { useEffect, useState, useCallback } from 'react';
import {
  Skeleton, App as AntApp, Button, Space, Tooltip, Popconfirm, Empty,
  Collapse, Switch,
} from 'antd';
import {
  ThunderboltOutlined,
  CopyOutlined,
  DeleteOutlined,
  EditOutlined,
  ExclamationCircleOutlined,
  CheckCircleOutlined,
  CloseCircleOutlined,
} from '@ant-design/icons';
import * as dbLoops from '@/utils/database/loops';
import type { LoopDetail } from '@/types/loop';
import { copyToClipboard } from '@/utils/clipboard';
import { getWorkspaceDisplayName, useProjectDirectories } from '@/utils/workspaceDisplay';
import { LoopFormModal } from './LoopFormModal';
import { LoopTriggersPanel, TRIGGER_META } from './LoopStudioTriggersPanel';
import { LoopStepsPanel } from './LoopStudioStepsPanel';
import { LoopExecutionsPanel } from './LoopStudioExecutionsPanel';

interface LoopDetailPanelProps {
  loopId: number;
  /** 可用标签列表（复用 Todo 的标签体系） */
  tags: Array<{ id: number; name: string; color: string }>;
  onTrigger: () => void;
  onDuplicate: () => void;
  onDelete: () => void;
  onToggleStatus: () => void;
  onChanged: () => void;
  hideTitleRow?: boolean;
}

export function LoopDetailPanel({
  loopId,
  tags,
  onTrigger,
  onDuplicate,
  onDelete,
  onToggleStatus,
  onChanged,
  hideTitleRow = false,
}: LoopDetailPanelProps) {
  const { message } = AntApp.useApp();
  const [detail, setDetail] = useState<LoopDetail | null>(null);
  const [loading, setLoading] = useState(true);
  // 基础信息编辑 modal 开关 (替代之前的 inline 编辑)
  const [editing, setEditing] = useState(false);
  // 工作空间目录（低基数集合，详情展示时把 path 转成 name 用）
  const { dirs: projectDirs } = useProjectDirectories();
  // 执行记录总数，由 LoopExecutionsPanel 通过回调更新
  const [executionTotal, setExecutionTotal] = useState(0);
  // 从 loop.limits_config 解析出的限制值，传递给子面板做兜底校验
  const [maxStepExecutions, setMaxStepExecutions] = useState<number | null>(null);
  const [maxTotalTokens, setMaxTotalTokens] = useState<number | null>(null);
  // 编辑弹窗预填数据，从 detail 提取
  const [editInitialData, setEditInitialData] = useState<{
    name: string; description: string; workspace_path: string | null;
    webhook_enabled: boolean;
    icon: string; review_template_id: number | null;
    tag_ids: number[]; limits_config: string | null;
    abnormal_handler_todo_id: number | null;
    abnormal_handler_trigger_on: string;
  } | null>(null);

  // 加载完整 detail, 子面板变更后也要重新拉以保持最新
  const reload = useCallback(() => {
    setLoading(true);
    dbLoops.getLoop(loopId)
      .then((d) => {
        setDetail(d);
        // 解析 limits_config 缓存限制值，传递给子面板做跳转自身时的兜底校验
        try {
          const lc = JSON.parse(d.limits_config || '{}');
          setMaxStepExecutions(lc.max_step_executions ?? null);
          setMaxTotalTokens(lc.max_total_tokens ?? null);
        } catch {
          // 忽略解析错误
        }
      })
      .catch(() => {
        message.error('加载 loop 详情失败');
        setDetail(null);
      })
      .finally(() => setLoading(false));
  }, [loopId, message]);

  useEffect(() => { reload(); }, [reload]);

  // 预加载执行记录总数（用于折叠标签展示，不等用户展开后才显示）
  useEffect(() => {
    dbLoops.listExecutions(loopId, { page: 1, limit: 1 })
      .then(res => setExecutionTotal(res.total))
      .catch(() => { /* 静默 */ });
  }, [loopId]);

  // 打开编辑 modal：从 detail 提取预填数据
  const handleOpenEdit = useCallback(() => {
    if (!detail) return;
    setEditInitialData({
      name: detail.name,
      description: detail.description,
      workspace_path: detail.workspace_path,
      webhook_enabled: detail.webhook_enabled,
      icon: detail.icon,
      review_template_id: detail.review_template_id ?? null,
      tag_ids: detail.tag_ids ?? [],
      limits_config: detail.limits_config,
      abnormal_handler_todo_id: detail.abnormal_handler_todo_id ?? null,
      abnormal_handler_trigger_on: detail.abnormal_handler_trigger_on ?? '["capped_step","capped_token","failed"]',
    });
    setEditing(true);
  }, [detail]);

  // 编辑保存后的回调：刷新详情 + 通知父组件
  const handleEditSaved = useCallback(() => {
    setEditing(false);
    setEditInitialData(null);
    reload();
    onChanged();
  }, [reload, onChanged]);

  if (loading && !detail) {
    return <Skeleton active style={{ padding: 24 }} />;
  }
  if (!detail) {
    return <Empty description="无法加载该 loop" style={{ marginTop: 64 }} />;
  }

  return (
    // 父容器已 overflow:auto, 这里只负责垂直 padding, 不再 height:100%
    <div className="loop-detail-panel detail-panel" style={{ padding: 'var(--space-xl)' }}>
      {!hideTitleRow && (
        <>
          {/* Header: 标签色条 + 标题 + 操作按钮 */}
          <div className="loop-detail-header" style={{ display: 'flex', alignItems: 'center', gap: 12, marginBottom: 16 }}>
            {(() => {
              const tag = tags.find(t => detail.tag_ids?.includes(t.id));
              return <span style={{ width: 4, height: 24, background: tag?.color || '#722ed1', borderRadius: 2 }} />;
            })()}
            <h2 style={{ margin: 0, fontSize: 18, flex: 1, minWidth: 0, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap', color: 'var(--color-text, #0f172a)' }}>
              {detail.name}
            </h2>
            <span style={{ color: 'var(--color-text-tertiary, #94a3b8)', fontSize: 12 }}>#{detail.id}</span>
            <Space size={4}>
              <Tooltip title="手动触发">
                <Button
                  size="small" type="primary"
                  icon={<ThunderboltOutlined />}
                  onClick={onTrigger}
                  disabled={detail.status !== 'enabled'}
                />
              </Tooltip>
              <Tooltip title="复制">
                <Button type="text" size="small" icon={<CopyOutlined />} onClick={onDuplicate} />
              </Tooltip>
              <Tooltip title="编辑">
                <Button type="text" size="small" icon={<EditOutlined />} onClick={handleOpenEdit} />
              </Tooltip>
              <Popconfirm
                title="删除 loop"
                description="将级联删除 triggers/steps,无法恢复"
                okType="danger"
                onConfirm={onDelete}
              >
                <Tooltip title="删除">
                  <Button type="text" size="small" icon={<DeleteOutlined />} />
                </Tooltip>
              </Popconfirm>
            </Space>
          </div>
        </>
      )}

      {detail.description && (
        <div style={{ color: 'var(--color-text-secondary, #475569)', fontSize: 13, marginBottom: 16 }}>{detail.description}</div>
      )}

      {/* Section: 基本信息 — 3 列布局, 与参考设计一致 */}
      <DetailSection title="基本信息">
        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(3, 1fr)', gap: 16 }}>
          <DetailField label="启用状态" value={
            <span style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
              <Switch
                checked={detail.status === 'enabled'}
                onChange={() => {
                  onToggleStatus();
                  // 切换后立即刷新详情, 让 Switch 和状态文字同步更新
                  setTimeout(() => { reload(); onChanged(); }, 100);
                }}
              />
              <span style={{
                fontSize: 12, fontWeight: 500,
                color: detail.status === 'enabled'
                  ? 'var(--color-success, #22c55e)'
                  : detail.status === 'paused'
                    ? 'var(--color-warning, #f59e0b)'
                    : 'var(--color-text-tertiary, #94a3b8)',
              }}>
                {detail.status === 'enabled' ? '已启用' : detail.status === 'paused' ? '已暂停' : '草稿'}
              </span>
            </span>
          } />
          <DetailField label="关联工作空间" value={
            detail.workspace_path ? (() => {
              const displayName = getWorkspaceDisplayName(projectDirs, detail.workspace_path);
              const dir = projectDirs.find(d => d.path === detail.workspace_path);
              return dir ? (
                <div>
                  <div style={{ fontWeight: 500 }}>{displayName}</div>
                  <div style={{ fontSize: 11, color: 'var(--color-text-tertiary, #94a3b8)', marginTop: 2 }}>
                    {dir.path}
                    {dir.git_worktree_enabled && (
                      <span style={{ marginLeft: 8 }}>· Git Worktree</span>
                    )}
                    {dir.auto_cleanup && (
                      <span style={{ marginLeft: 4 }}>· 自动清理</span>
                    )}
                  </div>
                </div>
              ) : (
                <span>{displayName}</span>
              );
            })() : <EmptyValue />
          } />
          <DetailField label="Webhook" value={
            detail.webhook_enabled ? (
              <span style={{ display: 'flex', alignItems: 'center', gap: 8, minWidth: 0 }}>
                <span
                  style={{
                    fontSize: 12,
                    fontWeight: 500,
                    color: 'var(--color-text, #0f172a)',
                    overflow: 'hidden',
                    textOverflow: 'ellipsis',
                    whiteSpace: 'nowrap',
                  }}
                  title={`${window.location.origin}/webhook/trigger/loop/${detail.id}`}
                >
                  {`${window.location.origin}/webhook/trigger/loop/${detail.id}`}
                </span>
                <Button
                  size="small"
                  icon={<CopyOutlined />}
                  onClick={async () => {
                    const ok = await copyToClipboard(`${window.location.origin}/webhook/trigger/loop/${detail.id}`);
                    if (ok) message.success('已复制 Webhook 地址');
                    else message.error('复制失败');
                  }}
                />
              </span>
            ) : (
              <span style={{ color: 'var(--color-text-tertiary, #94a3b8)' }}>未启用</span>
            )
          } />
          {/* 待人工审批提示 */}
          {detail.pending_approval_count > 0 && (
            <DetailField label="待审批" value={
              <span style={{
                display: 'inline-flex', alignItems: 'center', gap: 4,
                padding: '2px 10px', borderRadius: 12,
                background: 'var(--color-error-bg, #fef2f2)',
                color: 'var(--color-error, #ef4444)',
                fontWeight: 700, fontSize: 14,
              }}>
                <ExclamationCircleOutlined />
                {detail.pending_approval_count} 条待审批
              </span>
            } />
          )}
        </div>
      </DetailSection>

      {/* 全局限制 */}
      <DetailSection title="全局限制">
        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(3, 1fr)', gap: 16 }}>
          <DetailField label="最大执行步数" value={
            (() => {
              try {
                const lc = JSON.parse(detail.limits_config || '{}');
                return lc.max_step_executions != null
                  ? <span style={{ fontWeight: 500 }}>{lc.max_step_executions} 步</span>
                  : <span style={{ color: '#94a3b8' }}>未限制</span>;
              } catch { return <EmptyValue />; }
            })()
          } />
          <DetailField label="最大 Token 数" value={
            (() => {
              try {
                const lc = JSON.parse(detail.limits_config || '{}');
                return lc.max_total_tokens != null
                  ? <span style={{ fontWeight: 500 }}>{lc.max_total_tokens.toLocaleString()}</span>
                  : <span style={{ color: '#94a3b8' }}>未限制</span>;
              } catch { return <EmptyValue />; }
            })()
          } />
          <DetailField label="超限异常处理" value={
            (() => {
              const hasHandler = detail.abnormal_handler_todo_id != null;
              const triggerOn = detail.abnormal_handler_trigger_on ? JSON.parse(detail.abnormal_handler_trigger_on) : [];
              const hasCappedTrigger = Array.isArray(triggerOn) && (triggerOn.includes('capped_step') || triggerOn.includes('capped_token'));
              const enabled = hasHandler && hasCappedTrigger;
              return (
                <span style={{
                  display: 'inline-flex',
                  alignItems: 'center',
                  gap: 6,
                  padding: '2px 8px',
                  borderRadius: 12,
                  background: enabled ? '#dcfce7' : '#f1f5f9',
                  color: enabled ? '#166534' : '#64748b',
                  fontSize: 12,
                  fontWeight: 500,
                }}>
                  {enabled ? (
                    <CheckCircleOutlined style={{ fontSize: 12 }} />
                  ) : (
                    <CloseCircleOutlined style={{ fontSize: 12 }} />
                  )}
                  {enabled ? '已启用' : '未启用'}
                </span>
              );
            })()
          } />
        </div>
      </DetailSection>

      {/* 执行环节: DAG 流程图 */}
      <DetailSection title="执行环节" extra={
        <span style={{ fontSize: 11, color: 'var(--color-text-tertiary, #94a3b8)' }}>
          {detail.steps.length} 个环节按顺序执行
        </span>
      }>
        <LoopStepsPanel
          loopId={loopId}
          steps={detail.steps}
          onChanged={() => { reload(); onChanged(); }}
          maxStepExecutions={maxStepExecutions}
          maxTotalTokens={maxTotalTokens}
          workspacePath={detail.workspace_path}
        />
      </DetailSection>

      {/* 触发条件: 默认折叠, 仅展示摘要计数 */}
      <div style={{
        background: 'var(--color-bg-elevated, #ffffff)',
        border: '1px solid var(--color-border, #e2e8f0)',
        borderRadius: 8,
        marginBottom: 12,
        overflow: 'hidden',
      }}>
        <Collapse
          ghost
          expandIconPosition="end"
          defaultActiveKey={[]}
          items={[
            {
              key: 'triggers',
              label: (
                <span style={{ fontSize: 14, fontWeight: 600, color: 'var(--color-text, #0f172a)' }}>
                  触发条件
                  <span style={{ fontSize: 11, fontWeight: 400, color: 'var(--color-text-tertiary, #94a3b8)', marginLeft: 8 }}>
                    {detail.triggers.filter(t => t.enabled).length} / {Object.keys(TRIGGER_META).length} 已启用
                  </span>
                </span>
              ),
              children: (
                <div style={{ paddingTop: 4 }}>
                  <LoopTriggersPanel
                    loopId={loopId}
                    triggers={detail.triggers}
                    onChanged={() => { reload(); onChanged(); }}
                  />
                </div>
              ),
            },
          ]}
        />
      </div>

      {/* 折叠区: 执行历史 */}
      <div style={{
        background: 'var(--color-bg-elevated, #ffffff)',
        border: '1px solid var(--color-border, #e2e8f0)',
        borderRadius: 8,
        marginTop: 12,
        overflow: 'hidden',
      }}>
        <Collapse
          ghost
          expandIconPosition="end"
          defaultActiveKey={['executions']}
          items={[
            {
              key: 'executions',
              label: (
                <span style={{ fontSize: 14, fontWeight: 600, color: 'var(--color-text, #0f172a)' }}>
                  执行历史
                  {executionTotal > 0 && (
                    <span style={{ fontSize: 11, fontWeight: 400, color: 'var(--color-text-tertiary, #94a3b8)', marginLeft: 8 }}>
                      共 {executionTotal} 条
                    </span>
                  )}
                </span>
              ),
              children: (
                <div style={{ paddingTop: 4 }}>
                  <LoopExecutionsPanel loopId={loopId} loopName={detail.name} onTotalChange={setExecutionTotal} />
                </div>
              ),
            },
          ]}
        />
      </div>

      {/* 编辑基础信息 modal — 复用 LoopFormModal 组件 */}
      <LoopFormModal
        open={editing}
        mode="edit"
        loopId={loopId}
        initialData={editInitialData ?? undefined}
        tags={tags}
        onSaved={handleEditSaved}
        onClose={() => {
          setEditing(false);
          setEditInitialData(null);
        }}
      />
    </div>
  );
}

// 段标题 + 卡片容器
function DetailSection({ title, extra, children }: {
  title: string;
  extra?: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <section style={{
      background: 'var(--color-bg-elevated, #ffffff)',
      border: '1px solid var(--color-border, #e2e8f0)',
      borderRadius: 8,
      padding: 16,
      marginBottom: 12,
    }}>
      <div style={{
        display: 'flex', alignItems: 'center', gap: 8,
        marginBottom: 12,
        fontSize: 14, fontWeight: 600,
        color: 'var(--color-text, #0f172a)',
      }}>
        <span style={{ flex: 1 }}>{title}</span>
        {extra}
      </div>
      {children}
    </section>
  );
}

// 基本信息的一个字段 (label + value, 2 行)
function DetailField({ label, value }: { label: string; value: React.ReactNode }) {
  return (
    <div>
      <div style={{ fontSize: 11, color: 'var(--color-text-tertiary, #94a3b8)', marginBottom: 4 }}>{label}</div>
      <div style={{ fontSize: 13, color: 'var(--color-text, #0f172a)' }}>{value}</div>
    </div>
  );
}

// 空值占位
function EmptyValue() {
  return <span style={{ color: 'var(--color-text-tertiary, #94a3b8)' }}>—</span>;
}
