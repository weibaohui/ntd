// LoopListView — 028-列表详情独立路由：环路列表 table 形态。
//
// 设计要点（028-列表详情独立路由-设计 §4.4）：
// 1. 用 Ant Design `Table` 替代原 LoopListPanel 的卡片布局，单栏宽屏展示更多列。
// 2. 点击行 / 名称链接 → 调用 onSelectLoop 由父组件跳转到 `/#/loops/:id`。
// 3. 批量操作通过 `useBatchActions` hook 复用，与 TodoListView 共享同一套 Modal。
// 4. 单行操作（触发 / 复制 / 删除 / 启停状态）走 Dropdown 菜单，不挤占行宽。
// 5. 单函数 ≤ 30 行：列定义、行操作菜单、主渲染拆为独立函数。

import { useMemo, useState, type ReactNode } from 'react';
import { Button, Dropdown, Table, Tag } from 'antd';
import type { ColumnsType } from 'antd/es/table';
import {
  CopyOutlined,
  DeleteOutlined,
  MoreOutlined,
  PlayCircleOutlined,
  SettingOutlined,
} from '@ant-design/icons';
import { useApp } from '@/hooks/useApp';
import { useBatchActions } from '@/components/todo-list/useBatchActions'; // .tsx 含 JSX（批量 Modal）
import { formatRelativeTime } from '@/utils/datetime';
import type { Tag as TagType } from '@/types';
import type { LoopListItem } from '@/types/loop';

/** 环路状态 → 中文 + 颜色；与 LoopStudioDetailPanel 的状态展示保持一致。 */
const LOOP_STATUS_META: Record<string, { label: string; color: string }> = {
  enabled: { label: '已启用', color: 'success' },
  paused: { label: '已暂停', color: 'default' },
  disabled: { label: '已禁用', color: 'error' },
};

/** 把 status 串映射为 Tag；未知状态原样返回，便于扩展。 */
function renderLoopStatusTag(status?: string | null): ReactNode {
  if (!status) return '-';
  const meta = LOOP_STATUS_META[status];
  if (!meta) return status;
  return <Tag color={meta.color}>{meta.label}</Tag>;
}

/** 把 tag_ids 解析成 Tag 列表，最多展示 max 个，超出以 +N 显示。 */
function renderTagList(tagIds: number[] | undefined, tags: TagType[], max = 3): ReactNode {
  if (!tagIds || tagIds.length === 0) return '-';
  const resolved = tagIds
    .map(id => tags.find(t => t.id === id))
    .filter((t): t is TagType => !!t);
  if (resolved.length === 0) return '-';
  const visible = resolved.slice(0, max);
  const overflow = resolved.length - visible.length;
  return (
    <span style={{ display: 'inline-flex', gap: 4, flexWrap: 'wrap' }}>
      {visible.map(t => (
        <Tag key={t.id} color={t.color}>{t.name}</Tag>
      ))}
      {overflow > 0 && <Tag>+{overflow}</Tag>}
    </span>
  );
}

interface LoopListViewProps {
  /** 已经过滤后的环路列表（搜索由父组件完成）。 */
  items: LoopListItem[];
  /** 加载态：传入 true 时 table 显示 loading 蒙层。 */
  loading: boolean;
  /** 全量标签集（渲染 Tag 列用）。 */
  tags: TagType[];
  /** 当前行点击跳转：由父组件 pushUrl('loops', { id })。 */
  onSelectLoop: (id: number) => void;
  /** 单行触发入口（菜单「触发」）。 */
  onTrigger: (loop: LoopListItem) => void;
  /** 单行复制入口（菜单「复制」）。 */
  onDuplicate: (loop: LoopListItem) => void;
  /** 单行删除入口（菜单「删除」）。 */
  onDelete: (loop: LoopListItem) => void;
  /** 单行切换状态入口（菜单「启用/暂停」）。 */
  onToggleStatus: (loop: LoopListItem) => void;
  /** 操作成功后的刷新回调（批量操作完后触发）。 */
  onRefresh: () => void;
}

/**
 * 环路列表 table 视图。
 *
 * 整体处理思路：
 * 1. 接收已过滤的 items，渲染 Ant Design Table。
 * 2. 内部用 useBatchActions hook 管理批量 Modal（复制移动 / 强停）。
 * 3. 选中行通过 selectedIds 受控，触发顶部批量菜单。
 * 4. 单行操作走 Dropdown 菜单：触发 / 复制 / 启停 / 删除。
 */
export function LoopListView({
  items,
  loading,
  tags,
  onSelectLoop,
  onTrigger,
  onDuplicate,
  onDelete,
  onToggleStatus,
  onRefresh,
}: LoopListViewProps) {
  const { state } = useApp();
  const workspaceId = state.selectedWorkspace;
  // 行选中态：仅持有 id 列表，由 Table 的 rowSelection 受控
  const [selectedIds, setSelectedIds] = useState<number[]>([]);

  // 复用共享 hook：批量 Modal + 菜单项一次到位（loop 模式）
  const { batchActions, modals } = useBatchActions({
    mode: 'loop',
    selectedWorkspace: workspaceId,
    onRefreshLoops: onRefresh,
    onClearSelection: () => setSelectedIds([]),
  });

  // 单行操作菜单：触发 / 复制 / 切换状态 / 删除
  const buildRowActions = (loop: LoopListItem) => [
    {
      key: 'trigger',
      label: '触发',
      icon: <PlayCircleOutlined />,
      onClick: () => onTrigger(loop),
    },
    {
      key: 'duplicate',
      label: '复制',
      icon: <CopyOutlined />,
      onClick: () => onDuplicate(loop),
    },
    {
      key: 'toggle-status',
      label: loop.status === 'enabled' ? '暂停' : '启用',
      icon: <SettingOutlined />,
      onClick: () => onToggleStatus(loop),
    },
    {
      key: 'delete',
      label: '删除',
      icon: <DeleteOutlined />,
      danger: true,
      onClick: () => onDelete(loop),
    },
  ];

  // 列定义：抽为独立 useMemo，避免每次渲染重建
  const columns: ColumnsType<LoopListItem> = useMemo(() => [
    {
      title: 'ID',
      dataIndex: 'id',
      width: 70,
      fixed: 'left',
      render: (id: number) => (
        <span style={{ fontFamily: 'monospace', color: 'var(--color-text-tertiary)' }}>
          #{id}
        </span>
      ),
    },
    {
      title: '名称',
      dataIndex: 'name',
      ellipsis: true,
      render: (name: string, record) => (
        <a
          onClick={(e) => { e.stopPropagation(); onSelectLoop(record.id); }}
          style={{ color: 'var(--color-text)' }}
        >
          {name}
        </a>
      ),
    },
    {
      title: '状态',
      dataIndex: 'status',
      width: 100,
      render: renderLoopStatusTag,
    },
    {
      title: '标签',
      dataIndex: 'tag_ids',
      width: 180,
      render: (tagIds: number[]) => renderTagList(tagIds, tags),
    },
    {
      title: '环节',
      dataIndex: 'step_count',
      width: 70,
      align: 'center' as const,
    },
    {
      title: '触发次数',
      dataIndex: 'trigger_count',
      width: 80,
      align: 'center' as const,
    },
    {
      title: '待审批',
      dataIndex: 'pending_approval_count',
      width: 80,
      align: 'center' as const,
      render: (n: number) => (n > 0 ? <Tag color="warning">{n}</Tag> : '-'),
    },
    {
      title: '最近执行',
      dataIndex: 'last_execution_status',
      width: 100,
      render: (s?: string | null) => s ? <Tag color={s === 'success' ? 'success' : s === 'failed' ? 'error' : 'processing'}>{s}</Tag> : '-',
    },
    {
      title: '执行时间',
      dataIndex: 'last_execution_at',
      width: 130,
      render: (t?: string | null) => t ? formatRelativeTime(t) : '-',
    },
    {
      title: '更新时间',
      dataIndex: 'updated_at',
      width: 130,
      render: (t: string | null) => t ? formatRelativeTime(t) : '-',
    },
    {
      title: '操作',
      key: 'actions',
      width: 80,
      fixed: 'right' as const,
      render: (_, record) => (
        <Dropdown menu={{ items: buildRowActions(record) }} trigger={['click']}>
          <Button
            size="small"
            type="text"
            icon={<MoreOutlined />}
            onClick={(e) => e.stopPropagation()}
            aria-label="更多操作"
          />
        </Dropdown>
      ),
    },
  ], [tags, onSelectLoop, onTrigger, onDuplicate, onDelete, onToggleStatus]);

  // 整行点击跳转详情
  const handleRowClick = (record: LoopListItem) => ({
    onClick: () => onSelectLoop(record.id),
    style: { cursor: 'pointer' },
  });

  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%', overflow: 'hidden' }}>
      {/* 顶部工具栏：批量操作按钮（刷新/新建在 PageCard 顶部 header，避免重复） */}
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          gap: 8,
          padding: '6px 12px',
          borderBottom: '1px solid var(--color-border-light)',
          flexShrink: 0,
        }}
      >
        <BatchButton selectedIds={selectedIds} batchActions={batchActions} />
        <div style={{ flex: 1 }} />
        <span
          style={{
            fontSize: 12,
            color: 'var(--color-text-tertiary)',
          }}
        >
          已选 {selectedIds.length} 项
        </span>
      </div>

      {/* table 主体 */}
      <div style={{ flex: 1, minHeight: 0, overflow: 'auto' }}>
        <Table<LoopListItem>
          rowKey="id"
          columns={columns}
          dataSource={items}
          loading={loading}
          size="small"
          scroll={{ x: 1200 }}
          pagination={{
            pageSize: 20,
            showSizeChanger: true,
            pageSizeOptions: ['20', '50', '100'],
            showTotal: (total) => `共 ${total} 个环路`,
          }}
          rowSelection={{
            selectedRowKeys: selectedIds,
            onChange: (keys) => setSelectedIds(keys as number[]),
          }}
          onRow={handleRowClick}
        />
      </div>

      {/* 批量操作 Modal（由 useBatchActions 集中渲染） */}
      {modals}
    </div>
  );
}

/**
 * 批量按钮：受 selectedIds 控制，空选时禁用。
 * 抽为子组件让 LoopListView 主函数保持在 30 行内。
 */
function BatchButton({
  selectedIds,
  batchActions,
}: {
  selectedIds: number[];
  batchActions: { key: string; label: string; icon?: ReactNode; danger?: boolean; onClick: (ids: number[]) => void }[];
}) {
  if (batchActions.length === 0) return null;
  const disabled = selectedIds.length === 0;
  const items = batchActions.map(action => ({
    key: action.key,
    label: action.label,
    icon: action.icon,
    danger: action.danger,
    onClick: () => action.onClick(selectedIds),
    disabled,
  }));
  return (
    <Dropdown menu={{ items }} trigger={['click']} disabled={disabled}>
      <Button size="small" disabled={disabled} data-testid="loop-list-batch-trigger">
        批量 <MoreOutlined style={{ fontSize: 10 }} />
      </Button>
    </Dropdown>
  );
}
