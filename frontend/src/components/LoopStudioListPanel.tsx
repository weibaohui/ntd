// Loop Studio 左栏: 状态过滤 tab + loop 卡片列表。
//
// 设计要点 (对齐参考设计):
// - 顶部 tab 过滤: 全部 / 已启用 / 已暂停 / 草稿, 每个带计数
// - 卡片布局 (替代之前的纯文本行):
//   · 左侧 3px 颜色条 (loop.color)
//   · 名称 + 描述(产品) + 状态徽章
//   · 触发器数 / 阶段数 / 最近执行图标
//   · 底部 3px 进度条, 颜色按 last_execution_status 决定 (无则灰色)
// - 单击切换 selectedId, 父组件维护; 当前选中卡片左侧条加亮 + 边框高亮

import { useMemo, useState } from 'react';
import { Tag, Segmented } from 'antd';
import {
  ClockCircleOutlined,
  ApartmentOutlined,
  ThunderboltOutlined,
  CheckCircleOutlined,
  CloseCircleOutlined,
  LoadingOutlined,
  MinusCircleOutlined,
} from '@ant-design/icons';
import { formatRelativeTime } from '@/utils/datetime';
import type { LoopListItem, LoopStatus } from '@/types/loop';

type StatusFilter = 'all' | LoopStatus;

interface LoopListPanelProps {
  loops: LoopListItem[];
  selectedId: number | null;
  onSelect: (id: number) => void;
}

// 状态 → 标签颜色, 集中在一处方便复用
function statusTagColor(status: string): string {
  if (status === 'enabled') return 'green';
  if (status === 'paused') return 'orange';
  return 'default';
}

// 状态 → 中文标签, 与后端 LoopStatus enum 对齐
function statusLabel(status: string): string {
  if (status === 'enabled') return '已启用';
  if (status === 'paused') return '已暂停';
  return '草稿';
}

// 最近执行状态 → 底部进度条颜色, 使用主题变量让亮/暗主题自适应
function progressBarColor(status: string): string {
  if (status === 'success') return 'var(--color-success, #22c55e)';
  if (status === 'failed') return 'var(--color-error, #ef4444)';
  if (status === 'partial') return 'var(--color-warning, #f59e0b)';
  if (status === 'running') return 'var(--color-info, #3b82f6)';
  return 'var(--color-border, #e2e8f0)';
}

// 最近执行状态 → 图标, 使用主题变量, 暗色下也保持对比度
function executionIcon(status: string) {
  if (status === 'success') return <CheckCircleOutlined style={{ color: 'var(--color-success, #22c55e)' }} />;
  if (status === 'failed') return <CloseCircleOutlined style={{ color: 'var(--color-error, #ef4444)' }} />;
  if (status === 'partial') return <CloseCircleOutlined style={{ color: 'var(--color-warning, #f59e0b)' }} />;
  if (status === 'running') return <LoadingOutlined style={{ color: 'var(--color-info, #3b82f6)' }} />;
  return <MinusCircleOutlined style={{ color: 'var(--color-text-tertiary, #94a3b8)' }} />;
}

// 计算每个状态过滤的计数, 用于 segmented tab 显示「全部 4」这种格式
function countByStatus(loops: LoopListItem[], filter: StatusFilter): number {
  if (filter === 'all') return loops.length;
  return loops.filter(l => (l.status as LoopStatus) === filter).length;
}

export function LoopListPanel({ loops, selectedId, onSelect }: LoopListPanelProps) {
  // 状态过滤状态, 默认全部; 本地持有, 切 loop 时不重置
  const [filter, setFilter] = useState<StatusFilter>('all');

  // 按 id 倒序 (后端已按 updated_at desc, 再按 id 倒序保证新建后立刻显示在最前)
  const sorted = useMemo(
    () => [...loops].sort((a, b) => b.id - a.id),
    [loops],
  );

  // 应用状态过滤
  const filtered = useMemo(
    () => filter === 'all' ? sorted : sorted.filter(l => (l.status as LoopStatus) === filter),
    [sorted, filter],
  );

  return (
    <div
      className="loop-list-panel"
      style={{
        display: 'flex',
        flexDirection: 'column',
        flex: 1,
        minHeight: 0,
      }}
    >
      {/* 状态过滤 tab: 固定在顶部不滚动, 让卡片列表独占滚动 */}
      <div
        style={{
          flexShrink: 0,
          padding: '10px 12px',
          borderBottom: '1px solid var(--color-border, #e2e8f0)',
        }}
      >
        <Segmented
          block
          size="small"
          value={filter}
          onChange={(v) => setFilter(v as StatusFilter)}
          options={[
            { value: 'all', label: `全部 ${countByStatus(loops, 'all')}` },
            { value: 'enabled', label: `已启用 ${countByStatus(loops, 'enabled')}` },
            { value: 'paused', label: `已暂停 ${countByStatus(loops, 'paused')}` },
            { value: 'draft', label: `草稿 ${countByStatus(loops, 'draft')}` },
          ]}
        />
      </div>

      {/* 卡片列表: flex 1 占满剩余空间, 内部滚动 */}
      <div style={{ flex: 1, minHeight: 0, overflow: 'auto', padding: 8 }}>
        {filtered.length === 0 ? (
          <div style={{ padding: '40px 16px', textAlign: 'center', color: 'var(--color-text-tertiary, #94a3b8)', fontSize: 13 }}>
            该状态下暂无 loop
          </div>
        ) : (
          filtered.map(loop => (
            <LoopCard
              key={loop.id}
              loop={loop}
              selected={loop.id === selectedId}
              onClick={() => onSelect(loop.id)}
            />
          ))
        )}
      </div>
    </div>
  );
}

// 单张 loop 卡片: 颜色条 + 标题区 + meta + 底部进度条
function LoopCard({ loop, selected, onClick }: {
  loop: LoopListItem;
  selected: boolean;
  onClick: () => void;
}) {
  const status: LoopStatus = (loop.status as LoopStatus) ?? 'draft';
  // 副标题优先用 product (对齐参考设计「产品」字段), 缺则降级到 description
  const subtitle = loop.product || loop.description || '';

  return (
    <div
      className={`loop-card ${selected ? 'loop-card-selected' : ''}`}
      onClick={onClick}
      role="button"
      tabIndex={0}
      onKeyDown={(e) => {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault();
          onClick();
        }
      }}
      style={{
        position: 'relative',
        background: selected
          ? 'var(--color-primary-bg, #f0f9ff)'
          : 'var(--color-bg-elevated, #ffffff)',
        border: `1px solid ${selected
          ? 'var(--color-primary, #0891b2)'
          : 'var(--color-border, #e2e8f0)'}`,
        // 选中态用 inset 阴影代替外发光, 暗色下不刺眼
        boxShadow: selected
          ? 'inset 0 0 0 1px var(--color-primary, #0891b2)'
          : '0 1px 2px color-mix(in srgb, var(--color-text, #0f172a) 6%, transparent)',
        borderRadius: 10,
        padding: '12px 12px 14px 16px',
        marginBottom: 8,
        cursor: 'pointer',
        overflow: 'hidden',
        transition: 'background 200ms, border-color 200ms, box-shadow 200ms, transform 200ms',
      }}
      onMouseEnter={(e) => {
        if (!selected) {
          e.currentTarget.style.borderColor = 'var(--color-text-tertiary, #94a3b8)';
          e.currentTarget.style.boxShadow = '0 4px 10px color-mix(in srgb, var(--color-text, #0f172a) 10%, transparent)';
          e.currentTarget.style.transform = 'translateY(-1px)';
        }
      }}
      onMouseLeave={(e) => {
        if (!selected) {
          e.currentTarget.style.borderColor = 'var(--color-border, #e2e8f0)';
          e.currentTarget.style.boxShadow = '0 1px 2px color-mix(in srgb, var(--color-text, #0f172a) 6%, transparent)';
          e.currentTarget.style.transform = 'translateY(0)';
        }
      }}
    >
      {/* 左侧 3px 颜色条: 用 absolute 定位避免挤占 padding */}
      <span
        style={{
          position: 'absolute', left: 0, top: 0, bottom: 0, width: 3,
          background: loop.color || 'var(--color-primary, #0891b2)',
        }}
      />

      {/* 标题行: #id + 名称 + 状态徽章 */}
      <div style={{ display: 'flex', alignItems: 'center', gap: 6, marginBottom: 4 }}>
        <span style={{ color: 'var(--color-text-tertiary, #94a3b8)', fontSize: 11, fontFamily: 'monospace' }}>#{loop.id}</span>
        <span style={{
          fontWeight: 600, fontSize: 14, flex: 1, minWidth: 0,
          overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap',
          color: 'var(--color-text, #0f172a)',
        }}>
          {loop.name}
        </span>
        <Tag color={statusTagColor(status)} style={{ margin: 0, fontSize: 11, lineHeight: '18px', padding: '0 8px', borderRadius: 9 }}>
          {statusLabel(status)}
        </Tag>
      </div>

      {/* 副标题: 产品 / 描述, 缺则不渲染避免空白行 */}
      {subtitle && (
        <div style={{
          fontSize: 12, color: 'var(--color-text-secondary, #475569)', marginBottom: 8,
          overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap',
          lineHeight: 1.5,
        }}>
          {subtitle}
        </div>
      )}

      {/* meta: 触发器/环节数/最近执行 + 时间 */}
      <div style={{ display: 'flex', alignItems: 'center', gap: 12, fontSize: 11, color: 'var(--color-text-tertiary, #94a3b8)' }}>
        <span><ThunderboltOutlined /> {loop.trigger_count}</span>
        <span><ApartmentOutlined /> {loop.stage_count}</span>
        <span>{executionIcon(loop.last_execution_status)}</span>
        {loop.updated_at && (
          <span style={{ marginLeft: 'auto' }}>
            <ClockCircleOutlined /> {formatRelativeTime(loop.updated_at)}
          </span>
        )}
      </div>

      {/* 底部 3px 进度条: 颜色按最近执行状态, 无则灰色 24% 不透明感 */}
      <div
        style={{
          position: 'absolute', left: 0, right: 0, bottom: 0, height: 3,
          background: progressBarColor(loop.last_execution_status),
          opacity: loop.last_execution_status ? 1 : 0.35,
        }}
      />
    </div>
  );
}