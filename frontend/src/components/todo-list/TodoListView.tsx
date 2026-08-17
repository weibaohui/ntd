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
import type { MenuProps } from 'antd';
// antd Menu 项 onClick 的事件参数类型（含 domEvent），从公开 API 推导避免深层依赖 rc-menu
type MenuInfo = Parameters<NonNullable<MenuProps['onClick']>>[0];
import {
  ClockCircleOutlined,
  DeleteOutlined,
  EditOutlined,
  MoreOutlined,
  PlayCircleOutlined,
  ThunderboltOutlined,
} from '@ant-design/icons';
// 093：本组件只消费 todo 域状态，用细粒度 useTodos 替代合并版 useApp，
// 执行态（进度/统计推送）变化不再触发本组件重渲染。
import { useTodos } from '@/hooks/useTodoContext';
import { useResizableColumns } from '@/hooks/useResizableColumns';
import { useBatchActions } from './useBatchActions'; // .tsx 含 JSX（批量 Modal）
import { ExecutorBadge } from '@/components/ExecutorBadge';
import { ExpertBadge } from '@/components/ExpertBadge';
import { formatRelativeTime } from '@/utils/datetime';
import { formatProcessText } from '@/utils/processText';
import type { LoopRefSummary, TodoCenterItem } from '@/types';

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

/** 工艺列：展示引用该事项的环路所基于的工艺模板，格式 #模板ID-模板名-版本，按模板去重。 */
export function renderProcessColumn(refs: LoopRefSummary[] | undefined): ReactNode {
  if (!refs || refs.length === 0) return '-';
  // 多个环路可能基于同一模板，按 template_id 去重避免重复展示
  const seen = new Set<number>();
  const templates: { id: number; name?: string; version?: string }[] = [];
  for (const r of refs) {
    if (r.process_template_id == null) continue;
    if (seen.has(r.process_template_id)) continue;
    seen.add(r.process_template_id);
    templates.push({
      id: r.process_template_id,
      name: r.process_template_name,
      version: r.process_template_version,
    });
  }
  if (templates.length === 0) return '-';
  return (
    <span style={{ display: 'inline-flex', gap: 4, flexWrap: 'wrap' }}>
      {templates.map(t => (
        <Tag key={t.id}>{formatProcessText(t.id, t.name, t.version)}</Tag>
      ))}
    </span>
  );
}

/** 环路列：展示引用该事项的环路实例，格式 #环路ID 环路名。 */
export function renderLoopColumn(refs: LoopRefSummary[] | undefined): ReactNode {
  if (!refs || refs.length === 0) return '-';
  return (
    <span style={{ display: 'inline-flex', gap: 4, flexWrap: 'wrap' }}>
      {refs.map(r => (
        <Tag key={r.loop_id}>{`#${r.loop_id} ${r.loop_name}`}</Tag>
      ))}
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

/** 单行操作菜单项：每个动作 stopPropagation 防止触发行点击。
 *
 * 注意（冒泡陷阱）：Dropdown 菜单经 React Portal 渲染，合成事件会沿
 * React 组件树冒泡回表格行（即便菜单 DOM 挂在 body），只在触发按钮上
 * stopPropagation 挡不住「点菜单项 → 行 onClick → 误跳详情」。
 * 因此每个菜单项的 onClick 必须先 domEvent.stopPropagation()。
 * 导出供单元测试直接验证该防护不回归。 */
export function buildRowActionItems(
  todo: TodoCenterItem,
  callbacks: {
    onExecuteTodo: (t: TodoCenterItem) => void;
    onExecuteWithArgs: (t: TodoCenterItem) => void;
    onEditTodo: (t: TodoCenterItem) => void;
    onDeleteTodo: (t: TodoCenterItem) => void;
  },
) {
  // 包装回调：统一先挡冒泡再执行业务动作，避免每个菜单项重复书写
  const guard = (action: (t: TodoCenterItem) => void) => (info: MenuInfo) => {
    info.domEvent.stopPropagation();
    action(todo);
  };
  return [
    {
      key: 'execute',
      label: '执行一次',
      icon: <PlayCircleOutlined />,
      onClick: guard(callbacks.onExecuteTodo),
    },
    {
      key: 'execute-with-args',
      label: '带参执行',
      icon: <ThunderboltOutlined />,
      onClick: guard(callbacks.onExecuteWithArgs),
    },
    {
      key: 'edit',
      label: '编辑',
      icon: <EditOutlined />,
      onClick: guard(callbacks.onEditTodo),
    },
    {
      key: 'delete',
      label: '删除',
      icon: <DeleteOutlined />,
      danger: true,
      onClick: guard(callbacks.onDeleteTodo),
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
      // 056：服务端排序（sorter: true 仅展示排序指示，数据由后端排好返回）
      sorter: true,
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
      // 114：服务端排序——后端按类型 CASE 权重（评审>异常处理>快捷>事项）排序
      sorter: true,
      render: (_: unknown, record: TodoCenterItem) => {
        const td = record.todo_type ?? 0;
        const at = record.action_type;
        if (td === 2) return <Tag color="purple">评审</Tag>;
        if (td === 3) return <Tag color="magenta">异常处理</Tag>;
        if (at) return <Tag color="orange">快捷</Tag>;
        return <Tag color="blue">事项</Tag>;
      },
    },
    {
      title: '标题',
      dataIndex: 'title',
      // 必须给显式宽度：本表固定宽列合计已超 scroll.x，弹性列在窗口窄于
      // scroll.x 时会被压成 0 宽（整列「消失」，用户曾因此报标题列丢失）
      width: 220,
      ellipsis: true,
      // 056：服务端排序
      sorter: true,
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
      // 056：服务端排序
      sorter: true,
      render: renderStatusTag,
    },
    {
      title: '执行器',
      dataIndex: 'executor',
      width: 110,
      // 114：服务端排序（t.executor 字段，无执行器的行 DESC 时沉底）
      sorter: true,
      render: (executor?: string) =>
        executor ? <ExecutorBadge executor={executor} /> : '-',
    },
    {
      title: '专家',
      dataIndex: 'expert_name',
      width: 100,
      // 114：服务端排序（t.expert_name 字段）
      sorter: true,
      render: (name?: string | null) => name ? <ExpertBadge expertName={name} /> : '-',
    },
    {
      title: '调度',
      key: 'scheduler',
      width: 70,
      align: 'center',
      // 114：服务端排序（按 cron 配置存在性，有调度的行排前）
      sorter: true,
      render: (_, record) => renderSchedulerColumn(record),
    },
    {
      title: '环路',
      key: 'loop',
      width: 160,
      ellipsis: true,
      // 114：服务端排序（引用环路数，与展示口径同源）
      sorter: true,
      render: (_, record) => renderLoopColumn(record.referencing_loops),
    },
    {
      title: '工艺',
      key: 'process',
      width: 160,
      ellipsis: true,
      // 114：服务端排序（与环路列同源，按引用环路数）
      sorter: true,
      render: (_, record) => renderProcessColumn(record.referencing_loops),
    },
    {
      title: '最近执行',
      dataIndex: 'last_execution_status',
      width: 100,
      // 114：服务端排序——子查询取 MAX(id) 执行记录的状态（与展示口径一致）
      sorter: true,
      render: (status?: string | null) => renderStatusTag(status),
    },
    {
      title: '执行时间',
      dataIndex: 'last_execution_at',
      width: 130,
      // 114：恢复服务端排序——056 因聚合字段无法排序而移除，现由后端相关子查询
      // （MAX(id) 记录、finished_at 回退 started_at，与展示口径一致）支撑，跨页排序正确。
      sorter: true,
      render: (t?: string | null) => (t ? formatRelativeTime(t) : '-'),
    },
    {
      title: '更新时间',
      dataIndex: 'updated_at',
      width: 130,
      // 056：服务端排序
      sorter: true,
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
  /** 当前页数据（服务端分页/搜索/排序均在后端完成）。 */
  items: TodoCenterItem[];
  /** 加载态：传入 true 时 table 显示 loading 蒙层。 */
  loading: boolean;
  /** 服务端分页元数据。 */
  pagination: { current: number; pageSize: number; total: number };
  /** 翻页/改页大小/排序变化回调（父组件据此重新拉取对应页）。 */
  onServerChange: (page: number, pageSize: number, sortBy?: string, sortOrder?: 'asc' | 'desc') => void;
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
 * 3. 选中行通过 selectedIds 受控，触发顶部 BatchButton 的批量菜单（原 ActionToolbar 组件已删）。
 * 4. 单行操作走 Dropdown 菜单。
 */
export function TodoListView({
  items,
  loading,
  pagination,
  onServerChange,
  onSelectTodo,
  onEditTodo,
  onDeleteTodo,
  onExecuteTodo,
  onExecuteWithArgs,
  onRefresh,
}: TodoListViewProps) {
  const { state } = useTodos();
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

  // 列定义：useMemo 避免每次渲染重建。
  // 091：callbacks 必须先 memoize，否则每次渲染新建对象会让下方的 useMemo 依赖失效、
  // rawColumns 每次都重建（原代码注释说"避免重建"但 callbacks 内联对象恰恰破坏了它）。
  const callbacks = useMemo(
    () => ({ onSelectTodo, onExecuteTodo, onExecuteWithArgs, onEditTodo, onDeleteTodo }),
    [onSelectTodo, onExecuteTodo, onExecuteWithArgs, onEditTodo, onDeleteTodo],
  );
  const rawColumns = useMemo(() => buildTodoColumns(callbacks), [callbacks]);
  // 054：注入可拖拽列宽 + 受控排序 + localStorage 持久化。
  // 返回的 tableProps 包含 components / scroll / onChange，直接展开到 Table。
  const { columns, tableProps } = useResizableColumns('todos', rawColumns);

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
          // 054：scroll.x 由 hook 动态计算（列宽求和），替代原硬编码 1720
          {...tableProps}
          // 056：服务端分页/排序——onChange 把翻页与排序翻译给父组件重新拉取；
          // tableProps.onChange（排序偏好持久化）先执行，保持 localStorage 口径。
          onChange={(pag, filters, sorter, extra) => {
            tableProps.onChange?.(pag, filters, sorter, extra);
            const s = Array.isArray(sorter) ? sorter[0] : sorter;
            const sortOrder = s?.order === 'ascend' ? 'asc' : s?.order === 'descend' ? 'desc' : undefined;
            // 无 dataIndex 的列（「类型」列只有 key）sorter.field 为 undefined，
            // 回退 columnKey 才能把 sort_by=type 传给后端（114 修复）。
            const sortBy = s?.field != null ? String(s.field) : s?.columnKey != null ? String(s.columnKey) : undefined;
            onServerChange(
              pag.current ?? 1,
              pag.pageSize ?? 20,
              sortBy,
              sortOrder,
            );
          }}
          pagination={{
            current: pagination.current,
            pageSize: pagination.pageSize,
            total: pagination.total,
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
