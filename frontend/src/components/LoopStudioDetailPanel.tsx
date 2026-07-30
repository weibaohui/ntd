// Loop Studio 右栏 detail 容器（044 起只读化）。
//
// 对齐参考设计: 详情面板分成上下分段:
// - Header: 标题 + 操作 (仅删除；启停在「基本信息」的 Switch)
// - 基本信息: 启用 Switch + 工作空间
// - 执行环节: 横向卡片列表（按顺序执行）, 最重要放在最前（只读展示）
// - 执行历史: 折叠区, 默认收起 (不常用)
//
// 044（环路瘦身）：环路是工艺的运行时承载，触发/复制/导出/编辑/触发器配置
// 全部下线；定义变更只能改工艺 YAML 后重新 install/upgrade。

import { useEffect, useState, useCallback, useRef } from 'react';
import {
  Skeleton, App as AntApp, Empty,
  Collapse, Switch, Spin,
} from 'antd';
import {
  ExclamationCircleOutlined,
  CheckCircleOutlined,
  CloseCircleOutlined,
} from '@ant-design/icons';
import * as dbLoops from '@/utils/database/loops';
import type { LoopDetail } from '@/types/loop';
import { TraceBreadcrumb } from '@/components/common/TraceBreadcrumb';
import { getWorkspaceDisplayName, useProjectDirectories } from '@/utils/workspaceDisplay';
import { LoopStepsPanel } from './LoopStudioStepsPanel';
import { LoopExecutionsPanel } from './loop-studio/executions';
import { ProcessExecutionBoard } from '@/components/process/ProcessExecutionBoard';
// 删除按钮抽到 LoopDetailActions，与 LoopDetailPage 的 titleSuffix 共用。
import { LoopDetailActions } from './LoopDetailActions';
import type { LoopDetailActionsProps } from './LoopDetailActions';

interface LoopDetailPanelProps {
  loopId: number;
  /** 当前工作空间 ID（v1 路由 workspace-scoped，loop 查询必需） */
  workspaceId: number | null;
  /** 可用标签列表（复用 Todo 的标签体系） */
  tags: Array<{ id: number; name: string; color: string }>;
  onDelete: () => void;
  onToggleStatus: () => void;
  onChanged: () => void;
  /** 点击「来源工艺」跳转工艺详情（`/#/processes?name=xxx`）；未注入时行不可点击。 */
  onOpenProcess?: (templateName: string) => void;
  /** 点击流程图节点上的事项标题跳转事项详情；未注入时标题不可点击。 */
  onOpenTodo?: (todoId: number) => void;
  hideTitleRow?: boolean;
  /** 独立路由场景：把删除按钮上下文上报给外层 PageCard titleSuffix。
   *  hideTitleRow=true 时内层标题行（含按钮）整体隐藏，外层通过此回调拿到按钮上下文
   *  在 PageCard 标题行渲染删除按钮，避免按钮连带消失。 */
  onActionsReady?: (ctx: LoopDetailActionsProps | null) => void;
}

export function LoopDetailPanel({
  loopId,
  workspaceId,
  tags,
  onDelete,
  onToggleStatus,
  onChanged,
  onOpenProcess,
  onOpenTodo,
  hideTitleRow = false,
  onActionsReady,
}: LoopDetailPanelProps) {
  const { message: antMessage } = AntApp.useApp();
  const [detail, setDetail] = useState<LoopDetail | null>(null);
  const [loading, setLoading] = useState(true);
  // 工作空间目录（低基数集合，详情展示时把 path 转成 name 用）
  const { dirs: projectDirs } = useProjectDirectories();

  // 执行记录总数，由 LoopExecutionsPanel 通过回调更新
  const [executionTotal, setExecutionTotal] = useState(0);

  // 防切换竞态：ref 始终持有最新 loopId。reload 与 executionTotal 的请求 resolve 后
  // 与 ref 比较，不一致说明期间已切到别的 loop，丢弃 stale 响应避免覆盖新 loop 的数据。
  const latestLoopIdRef = useRef(loopId);
  latestLoopIdRef.current = loopId;

  // 加载完整 detail, 子面板变更后也要重新拉以保持最新
  const reload = useCallback(() => {
    // 捕获本次请求所属的 loopId，resolve 后与最新值比较
    const id = loopId;
    setLoading(true);
    dbLoops.getLoop(workspaceId ?? 0, id)
      .then((d) => {
        if (latestLoopIdRef.current !== id) return; // 已切换到别的 loop，丢弃
        setDetail(d);
      })
      .catch(() => {
        if (latestLoopIdRef.current !== id) return; // 切换后的错误不弹窗
        // 只提示错误，不清空已加载的 detail；保留最后一次成功加载的数据供用户查看
        antMessage.error('加载 loop 详情失败');
      })
      .finally(() => {
        if (latestLoopIdRef.current === id) setLoading(false);
      });
  }, [loopId, workspaceId, antMessage]);

  useEffect(() => { reload(); }, [reload]);


  // 预加载执行记录总数（用于折叠标签展示，不等用户展开后才显示）
  useEffect(() => {
    // 捕获本次 loopId，resolve 后比较，丢弃切换后的 stale 响应
    const id = loopId;
    dbLoops.listExecutions(workspaceId ?? 0, id, { page: 1, limit: 1 })
      .then(res => { if (latestLoopIdRef.current === id) setExecutionTotal(res.total); })
      .catch(() => { /* 静默 */ });
  }, [loopId]);

  // 独立路由场景：把删除按钮上下文上报给外层 PageCard 的 titleSuffix。
  // detail 为空时上报 null（加载中），外层相应不渲染按钮。
  useEffect(() => {
    if (!onActionsReady) return;
    if (!detail) {
      onActionsReady(null);
      return;
    }
    onActionsReady({ onDelete });
  }, [detail, onDelete, onActionsReady]);

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
          {/* Header: 标签色条 + 标题 + 删除按钮（044：触发/复制/导出/编辑已下线） */}
          <div className="loop-detail-header" style={{ display: 'flex', alignItems: 'center', gap: 12, marginBottom: 16 }}>
            {(() => {
              const tag = tags.find(t => detail.tag_ids?.includes(t.id));
              return <span style={{ width: 4, height: 24, background: tag?.color || '#722ed1', borderRadius: 2 }} />;
            })()}
            <h2 style={{ margin: 0, fontSize: 18, flex: 1, minWidth: 0, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap', color: 'var(--color-text, #0f172a)' }}>
              {detail.name}
            </h2>
            <span style={{ color: 'var(--color-text-tertiary, #94a3b8)', fontSize: 12 }}>#{detail.id}</span>
            <LoopDetailActions onDelete={onDelete} />
          </div>
        </>
      )}

      {detail.process_template_id != null && detail.process_template_name && (
        <div data-testid="loop-source-process">
          <TraceBreadcrumb
            title="来源工艺"
            segments={[{
              label: detail.process_template_display_name || detail.process_template_name,
              techName: detail.process_template_name,
              version: detail.process_template_version || undefined,
              // 040：回跳按 guid 寻址（name 可重复）；旧环路无 guid 时回退 name 让链接不失效。
              onClick: onOpenProcess
                ? () => onOpenProcess(detail.process_template_guid ?? detail.process_template_name!)
                : undefined,
            }]}
          />
        </div>
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
            detail.workspace_id != null ? (() => {
              const displayName = getWorkspaceDisplayName(projectDirs, detail.workspace_id);
              const dir = projectDirs.find(d => d.id === detail.workspace_id);
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
              // 异常处理启用判定：以工艺定义的 prompt 是否存在为准（需求 035）
              const hasHandler = detail.abnormal_handler_prompt != null && detail.abnormal_handler_prompt !== '';
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

      {/* 执行环节: DAG 流程图（044 起只读，定义变更请改工艺 YAML） */}
      <DetailSection title="执行环节" extra={
        <span style={{ fontSize: 11, color: 'var(--color-text-tertiary, #94a3b8)' }}>
          {detail.steps.length} 个环节按顺序执行
        </span>
      }>
        <LoopStepsPanel
          steps={detail.steps}
          onOpenTodo={onOpenTodo}
        />
      </DetailSection>

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
          expandIconPlacement="end"
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
                  <LoopExecutionsPanel loopId={loopId} workspaceId={detail.workspace_id} loopName={detail.name} onTotalChange={setExecutionTotal} />
                </div>
              ),
            },
            // 工艺执行看板：仅当环路是工艺实例且有执行记录时展示，取最新一条执行。
            ...(detail.process_template_id != null && executionTotal > 0 ? [{
              key: 'execution-board',
              label: '执行看板',
              children: (
                <LatestExecBoard workspaceId={detail.workspace_id} loopId={loopId} />
              ),
            }] : []),
          ]}
        />
      </div>
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

// 执行看板包装组件：自动取最新一条执行记录并渲染 ProcessExecutionBoard。
// 定义在模块顶层确保 hook 规则合规（嵌套组件内不可用 hook）。
function LatestExecBoard({ workspaceId, loopId }: { workspaceId: number | null; loopId: number }) {
  const [execId, setExecId] = useState<number | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    if (!workspaceId) return;
    let cancelled = false;
    dbLoops.listExecutions(workspaceId, loopId, { limit: 1 })
      .then(res => {
        if (!cancelled && res.items.length > 0) {
          setExecId(res.items[0].id);
        }
      })
      .finally(() => { if (!cancelled) setLoading(false); });
    return () => { cancelled = true; };
  }, [workspaceId, loopId]);

  if (loading) return <Spin style={{ display: 'block', margin: '20px auto' }} />;
  if (execId == null || !workspaceId) return <Empty description="暂无执行记录" />;

  return <ProcessExecutionBoard workspaceId={workspaceId} loopId={loopId} executionId={execId} />;
}

// 空值占位
function EmptyValue() {
  return <span style={{ color: 'var(--color-text-tertiary, #94a3b8)' }}>—</span>;
}
