// TodoListView — 028-列表详情独立路由：事项列表 table 形态。
//
// 设计要点（028-列表详情独立路由-设计 §4.3）：
// 1. 用 Ant Design `Table` 替代原 `TodoList` 的卡片式行布局，单栏宽屏展示更多列。
// 2. 点击行 / 标题链接 → 调用 onSelectTodo 由父组件跳转到 `/#/todos/:id`。
// 3. 批量操作通过 `useBatchActions` hook 复用，与 LoopListView 共享同一套 Modal。
// 4. 单行操作（执行 / 编辑 / 删除）走 Dropdown 菜单，不挤占行宽。
// 5. 单函数 ≤ 30 行：列定义、行操作菜单、主渲染拆为独立函数。
//
// 数据由父组件（TodoListPage）注入，本组件不直接拉接口，便于测试与复用。

import { useEffect, useMemo, useState, type ReactNode } from 'react';
import { Button, Dropdown, Table, Tag, Tooltip } from 'antd';
import type { ColumnsType } from 'antd/es/table';
import {
  ClockCircleOutlined,
  DeleteOutlined,
  EditOutlined,
  MoreOutlined,
  PlayCircleOutlined,
  ThunderboltOutlined,
} from '@ant-design/icons';
import { useApp } from '@/hooks/useApp';
import { useBatchActions } from './useBatchActions'; // .tsx 含 JSX（批量 Modal）
import { ExecutorBadge } from '@/components/ExecutorBadge';
import { ExpertBadge } from '@/components/ExpertBadge';
import { formatRelativeTime } from '@/utils/datetime';
import type { Tag as TagType, TodoCenterItem } from '@/types';

/** 状态 → 中文 + 颜色映射；与事项中心卡片 StatusTag 保持一致口径。 */
const STATUS_META: Record<string, { label: string; color: string }> = {
  pending: { label: '待执行', color: 'default' },
  running: { label: '运行中', color: 'processing' },
  completed: { label: '已完成', color: 'success' },
  failed: { label: '失败', color: 'error' },
};

/** 把后端 status 串映射为 Tag 展示；未知状态返回 null 避免渲染噪音。 */
function renderStatusTag(status?: string | null): ReactNode {
  if (!status) return '-';
  const meta = STATUS_META[status];
  if (!meta) return status;
  return <Tag color={meta.color}>{meta.label}</Tag>;
}

/** 把 tag_ids 解析成实际 Tag 列表，最多展示 max 个，超出以 +N 显示。 */
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

/** 调度器列：仅显示图标，鼠标 hover 展示 cron 表达式。 */
function renderSchedulerColumn(record: TodoCenterItem): ReactNode {
  if (!record.scheduler_config) return '-';
  const color = record.scheduler_enabled ? 'var(--color-warning)' : 'var(--color-text-tertiary)';
  return (
    <Tooltip title={record.scheduler_config}>
      <ClockCircleOutlined style={{ color }} />
    </Tooltip>
  );
}

/** 单行操作菜单项：每个动作 stopPropagation 防止触发行点击。 */
function buildRowActionItems(
  todo: TodoCenterItem,
  callbacks: {
    onExecuteTodo: (t: TodoCenterItem) => void;
    onExecuteWithArgs: (t: TodoCenterItem) => void;
    onEditTodo: (t: TodoCenterItem) => void;
    onDeleteTodo: (t: TodoCenterItem) => void;
  },
) {
  return [
    {
      key: 'execute',
      label: '执行一次',
      icon: <PlayCircleOutlined />,
      onClick: () => callbacks.onExecuteTodo(todo),
    },
    {
      key: 'execute-with-args',
      label: '带参执行',
      icon: <ThunderboltOutlined />,
      onClick: () => callbacks.onExecuteWithArgs(todo),
    },
    {
      key: 'edit',
      label: '编辑',
      icon: <EditOutlined />,
      onClick: () => callbacks.onEditTodo(todo),
    },
    {
      key: 'delete',
      label: '删除',
      icon: <DeleteOutlined />,
      danger: true,
      onClick: () => callbacks.onDeleteTodo(todo),
    },
  ];
}

/** 行操作列渲染函数：Dropdown 菜单包裹。 */
function renderActionsColumn(
  record: TodoCenterItem,
  callbacks: Parameters<typeof buildRowActionItems>[1],
): ReactNode {
  return (
    <Dropdown menu={{ items: buildRowActionItems(record, callbacks) }} trigger={['click']}>
      <Button
        size="small"
        type="text"
        icon={<MoreOutlined />}
        onClick={(e) => e.stopPropagation()}
        aria-label="更多操作"
      />
    </Dropdown>
  );
}

// 构建 Table 列定义：抽为独立函数，让组件主体保持在 30 行内。
function buildTodoColumns(
  tags: TagType[],
  callbacks: {
    onSelectTodo: (id: number) => void;
  } & Parameters<typeof buildRowActionItems>[1],
): ColumnsType<TodoCenterItem> {
  return [
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
      title: '类型',
      key: 'type',
      width: 100,
      render: (_: unknown, record: TodoCenterItem) => {
        const td = record.todo_type ?? 0;
        const at = record.action_type;
        if (td === 2) return <Tag color="purple">评审</Tag>;
        if (at) return <Tag color="orange">快捷</Tag>;
        return <Tag color="blue">事项</Tag>;
      },
    },
    {
      title: '标题',
      dataIndex: 'title',
      ellipsis: true,
      render: (title: string, record) => (
        <a
          onClick={(e) => { e.stopPropagation(); callbacks.onSelectTodo(record.id); }}
          style={{ color: 'var(--color-text)' }}
        >
          {title}
        </a>
      ),
    },
    {
      title: '状态',
      dataIndex: 'status',
      width: 100,
      render: renderStatusTag,
    },
    {
      title: '标签',
      dataIndex: 'tag_ids',
      width: 180,
      render: (tagIds: number[]) => renderTagList(tagIds, tags),
    },
    {
      title: '执行器',
      dataIndex: 'executor',
      width: 110,
      render: (executor?: string) =>
        executor ? <ExecutorBadge executor={executor} /> : '-',
    },
    {
      title: '专家',
      dataIndex: 'expert_name',
      width: 100,
      render: (name?: string | null) => name ? <ExpertBadge expertName={name} /> : '-',
    },
    {
      title: '调度',
      key: 'scheduler',
      width: 70,
      align: 'center',
      render: (_, record) => renderSchedulerColumn(record),
    },
    {
      title: '最近执行',
      dataIndex: 'last_execution_status',
      width: 100,
      render: (status?: string | null) => renderStatusTag(status),
    },
    {
      title: '执行时间',
      dataIndex: 'last_execution_at',
      width: 130,
      render: (t?: string | null) => (t ? formatRelativeTime(t) : '-'),
    },
    {
      title: '更新时间',
      dataIndex: 'updated_at',
      width: 130,
      render: (t: string) => formatRelativeTime(t),
    },
    {
      title: '操作',
      key: 'actions',
      width: 80,
      fixed: 'right',
      render: (_, record) => renderActionsColumn(record, callbacks),
    },
  ];
}

/** 行点击跳转详情；操作列已 stopPropagation 防误触。 */
function handleRowClick(onSelectTodo: (id: number) => void, record: TodoCenterItem) {
  return {
    onClick: () => onSelectTodo(record.id),
    style: { cursor: 'pointer' as const },
  };
}

interface TodoListViewProps {
  /** 已经过滤后的列表数据（搜索/标签筛选由父组件完成）。 */
  items: TodoCenterItem[];
  /** 加载态：传入 true 时 table 显示 loading 蒙层。 */
  loading: boolean;
  /** 全量标签集（渲染 Tag 列用）。 */
  tags: TagType[];
  /** 当前行点击跳转：由父组件 pushUrl('todos', { id })。 */
  onSelectTodo: (id: number) => void;
  /** 编辑事项入口（单行菜单「编辑」触发）。 */
  onEditTodo: (todo: TodoCenterItem) => void;
  /** 删除事项入口（单行菜单「删除」触发）。 */
  onDeleteTodo: (todo: TodoCenterItem) => void;
  /** 执行事项入口（单行菜单「执行」触发）。 */
  onExecuteTodo: (todo: TodoCenterItem) => void;
  /** 带参数执行入口（单行菜单「带参执行」触发）。 */
  onExecuteWithArgs: (todo: TodoCenterItem) => void;
  /** 操作成功后的刷新回调（批量操作完后触发）。 */
  onRefresh: () => void;
}

/** 选中 ID 裁剪 hook：items 变化时移除已消失的行。 */
function useSelectedIdsClipping(items: TodoCenterItem[]) {
  const [selectedIds, setSelectedIds] = useState<number[]>([]);
  useEffect(() => {
    // items 变化时清理已不存在的选中 id，避免对删除行发起批量操作
    setSelectedIds(prev => {
      const alive = prev.filter(id => items.some(i => i.id === id));
      return alive.length === prev.length ? prev : alive;
    });
  }, [items]);
  return { selectedIds, setSelectedIds };
}

/**
 * 事项列表 table 视图。
 *
 * 整体处理思路：
 * 1. 接收已过滤的 items，渲染 Ant Design Table。
 * 2. 内部用 useBatchActions hook 管理批量 Modal。
 * 3. 选中行通过 selectedIds 受控，触发顶部 ActionToolbar 的批量菜单。
 * 4. 单行操作走 Dropdown 菜单。
 */
export function TodoListView({
  items,
  loading,
  tags,
  onSelectTodo,
  onEditTodo,
  onDeleteTodo,
  onExecuteTodo,
  onExecuteWithArgs,
  onRefresh,
}: TodoListViewProps) {
  const { state } = useApp();
  const workspaceId = state.selectedWorkspace;
  // 行选中态裁剪：items 变化时清掉已消失行
  const { selectedIds, setSelectedIds } = useSelectedIdsClipping(items);

  // 复用共享 hook：批量 Modal + 菜单项
  const { batchActions, modals } = useBatchActions({
    mode: 'item',
    selectedWorkspace: workspaceId,
    onRefreshItems: onRefresh,
    onClearSelection: () => setSelectedIds([]),
  });

  // 列定义：useMemo 避免每次渲染重建
  const callbacks = { onSelectTodo, onExecuteTodo, onExecuteWithArgs, onEditTodo, onDeleteTodo };
  const columns = useMemo(() => buildTodoColumns(tags, callbacks), [tags, callbacks]);

  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%', overflow: 'hidden' }}>
      {/* 顶部工具栏 */}
      <div style={{
        display: 'flex', alignItems: 'center', gap: 8,
        padding: '6px 12px', borderBottom: '1px solid var(--color-border-light)', flexShrink: 0,
      }}>
        <BatchButton selectedIds={selectedIds} batchActions={batchActions} />
        <div style={{ flex: 1 }} />
        <span style={{ fontSize: 12, color: 'var(--color-text-tertiary)' }}>
          已选 {selectedIds.length} 项
        </span>
      </div>

      {/* table 主体 */}
      <div style={{ flex: 1, minHeight: 0, overflow: 'auto' }}>
        <Table<TodoCenterItem>
          rowKey="id"
          columns={columns}
          dataSource={items}
          loading={loading}
          size="small"
          scroll={{ x: 1300 }}
          pagination={{
            pageSize: 20,
            showSizeChanger: true,
            pageSizeOptions: ['20', '50', '100'],
            showTotal: (total) => `共 ${total} 项`,
          }}
          rowSelection={{
            selectedRowKeys: selectedIds,
            onChange: (keys) => setSelectedIds(keys as number[]),
          }}
          onRow={(record) => handleRowClick(onSelectTodo, record)}
        />
      </div>

      {modals}
    </div>
  );
}

/**
 * 批量按钮：受 selectedIds 控制，空选时禁用。
 * 抽为子组件让 TodoListView 主函数保持在 30 行内。
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
      <Button size="small" disabled={disabled} data-testid="todo-list-batch-trigger">
        批量 <MoreOutlined style={{ fontSize: 10 }} />
      </Button>
    </Dropdown>
  );
}
