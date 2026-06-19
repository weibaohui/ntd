// Loop 执行历史面板。
//
// 展示 loop 的运行历史, 每行是一次 execution (一次完整 loop 调用)：
// - id / 触发类型 / 开始时间 / 耗时 / 状态 / 阶段完成进度
// - 点击行展开看 stage_executions 明细
//
// 分页: page + limit, 简单表格不引入分页器, 改成"加载更多"按钮避免侵入式 UI

import { useState, useEffect, useCallback } from 'react';
import { App as AntApp, Button, Empty, Skeleton, Tag, Space, Tooltip, Descriptions, Collapse } from 'antd';
import {
  CheckCircleOutlined,
  CloseCircleOutlined,
  LoadingOutlined,
  MinusCircleOutlined,
  ReloadOutlined,
  HistoryOutlined,
} from '@ant-design/icons';
import * as dbLoops from '@/utils/database/loops';
import type { LoopExecutionDto, LoopExecutionDetail } from '@/types/loop';
import { formatRelativeTime } from '@/utils/datetime';

interface Props {
  loopId: number;
  loopName: string;
}

const DEFAULT_PAGE_LIMIT = 20;

// 状态 → 颜色 + 图标, 与 LoopListPanel.executionIcon 保持一致
function execStatusView(status: string): { color: string; icon: React.ReactNode; label: string } {
  switch (status) {
    case 'success': return { color: 'green', icon: <CheckCircleOutlined />, label: '成功' };
    case 'failed': return { color: 'red', icon: <CloseCircleOutlined />, label: '失败' };
    case 'partial': return { color: 'orange', icon: <CloseCircleOutlined />, label: '部分' };
    case 'running': return { color: 'blue', icon: <LoadingOutlined />, label: '运行中' };
    case 'cancelled': return { color: 'default', icon: <MinusCircleOutlined />, label: '已取消' };
    default: return { color: 'default', icon: <MinusCircleOutlined />, label: status };
  }
}

// 计算耗时
function durationLabel(start: string, end: string | null): string {
  if (!end) return '进行中';
  const ms = new Date(end).getTime() - new Date(start).getTime();
  if (ms < 1000) return `${ms}ms`;
  if (ms < 60_000) return `${(ms / 1000).toFixed(1)}s`;
  return `${Math.floor(ms / 60_000)}m ${Math.floor((ms % 60_000) / 1000)}s`;
}

export function LoopExecutionsPanel({ loopId, loopName }: Props) {
  const { message } = AntApp.useApp();
  const [items, setItems] = useState<LoopExecutionDto[]>([]);
  const [total, setTotal] = useState(0);
  const [page, setPage] = useState(1);
  const [loading, setLoading] = useState(false);
  const [expandedId, setExpandedId] = useState<number | null>(null);
  const [expandedDetail, setExpandedDetail] = useState<LoopExecutionDetail | null>(null);
  const [expandedLoading, setExpandedLoading] = useState(false);

  // 加载一页执行记录
  const loadPage = useCallback((p: number) => {
    setLoading(true);
    dbLoops.listExecutions(loopId, { page: p, limit: DEFAULT_PAGE_LIMIT })
      .then((res) => {
        if (p === 1) {
          setItems(res.items);
        } else {
          setItems(prev => [...prev, ...res.items]);
        }
        setTotal(res.total);
        setPage(p);
      })
      .catch(() => {
        if (p === 1) setItems([]);
      })
      .finally(() => setLoading(false));
  }, [loopId]);

  useEffect(() => { loadPage(1); }, [loadPage]);

  // 展开行: 拉取该 execution 的 stage 详情
  const handleExpand = useCallback(async (execId: number) => {
    if (expandedId === execId) {
      setExpandedId(null);
      setExpandedDetail(null);
      return;
    }
    setExpandedId(execId);
    setExpandedLoading(true);
    try {
      const detail = await dbLoops.getExecution(loopId, execId);
      setExpandedDetail(detail);
    } catch {
      message.error('加载执行详情失败');
      setExpandedId(null);
    } finally {
      setExpandedLoading(false);
    }
  }, [expandedId, loopId, message]);

  // 加载更多
  const handleLoadMore = useCallback(() => {
    if (items.length < total && !loading) {
      loadPage(page + 1);
    }
  }, [items.length, total, loading, page, loadPage]);

  return (
    <div className="loop-executions-panel">
      <div style={{ marginBottom: 12, display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
        <Space>
          <HistoryOutlined />
          <span style={{ color: 'var(--color-text-secondary, #475569)' }}>{loopName} 的执行历史 (共 {total} 条)</span>
        </Space>
        <Button size="small" icon={<ReloadOutlined />} onClick={() => loadPage(1)}>
          刷新
        </Button>
      </div>

      {loading && items.length === 0 ? (
        <Skeleton active />
      ) : items.length === 0 ? (
        <Empty description="暂无执行记录; 启用 loop 后触发一次即可看到" />
      ) : (
        <>
          {items.map(e => {
            const view = execStatusView(e.status);
            const expanded = expandedId === e.id;
            return (
              <div key={e.id} className="loop-exec-row">
                <div
                  className="loop-exec-row-head"
                  onClick={() => handleExpand(e.id)}
                  style={{ cursor: 'pointer' }}
                >
                  <Space>
                    {view.icon}
                    <span>#{e.id}</span>
                    <Tag color={view.color}>{view.label}</Tag>
                    <Tag>{e.trigger_type}</Tag>
                    <Tooltip title={`开始: ${e.started_at}`}>
                      <span style={{ color: 'var(--color-text-tertiary, #94a3b8)' }}>{formatRelativeTime(e.started_at)}</span>
                    </Tooltip>
                    <span style={{ color: 'var(--color-text-secondary, #475569)' }}>
                      {e.completed_stages}/{e.total_stages} 阶段
                    </span>
                    {e.failed_stages > 0 && (
                      <Tag color="red">{e.failed_stages} 失败</Tag>
                    )}
                    <span style={{ color: 'var(--color-text-tertiary, #94a3b8)', fontSize: 12 }}>
                      耗时 {durationLabel(e.started_at, e.finished_at)}
                    </span>
                  </Space>
                </div>
                {expanded && (
                  <div className="loop-exec-row-detail">
                    {expandedLoading ? (
                      <Skeleton active />
                    ) : expandedDetail && expandedDetail.id === e.id ? (
                      <StageExecList stageExecs={expandedDetail.stage_executions} />
                    ) : null}
                  </div>
                )}
              </div>
            );
          })}
          {items.length < total && (
            <div style={{ textAlign: 'center', marginTop: 12 }}>
              <Button onClick={handleLoadMore} loading={loading}>
                加载更多 ({items.length}/{total})
              </Button>
            </div>
          )}
        </>
      )}
    </div>
  );
}

// 阶段执行明细, 单独抽出来便于阅读
function StageExecList({ stageExecs }: { stageExecs: Record<string, any>[] }) {
  if (stageExecs.length === 0) {
    return <Empty description="无阶段执行记录" />;
  }
  return (
    <Collapse
      size="small"
      items={stageExecs.map(s => ({
        key: s.id,
        label: (
          <Space>
            {execStatusView(s.status).icon}
            <span>阶段 #{s.stage_id}</span>
            <Tag color={execStatusView(s.status).color}>{s.status}</Tag>
            {s.started_at && (
              <span style={{ color: 'var(--color-text-tertiary, #94a3b8)', fontSize: 12 }}>
                {durationLabel(s.started_at, s.finished_at)}
              </span>
            )}
          </Space>
        ),
        children: (
          <Descriptions size="small" column={1}>
            <Descriptions.Item label="阶段 ID">{s.stage_id}</Descriptions.Item>
            <Descriptions.Item label="Todo ID">{s.todo_id}</Descriptions.Item>
            <Descriptions.Item label="执行记录">
              {s.execution_record_id ? `#${s.execution_record_id}` : '-'}
            </Descriptions.Item>
            <Descriptions.Item label="开始">{s.started_at ?? '-'}</Descriptions.Item>
            <Descriptions.Item label="结束">{s.finished_at ?? '-'}</Descriptions.Item>
            {s.error_message && (
              <Descriptions.Item label="错误">
                <span style={{ color: 'var(--color-error, #ef4444)' }}>{s.error_message}</span>
              </Descriptions.Item>
            )}
          </Descriptions>
        ),
      }))}
    />
  );
}
