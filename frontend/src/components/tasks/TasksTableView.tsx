// 任务列表视图：AntD Table 单表格 + 自带筛选 toolbar。
// 形态参考 ItemsPage 列表态的双栏联动：
//   左侧 Table 行可点击选中任务，触发宿主右栏渲染 TaskDetailPanel。

import { useEffect, useMemo, useRef, useState } from 'react';
import { Button, Dropdown, Table, Tag, Typography, Select, Empty, App as AntApp } from 'antd';
import { MoreOutlined, DeleteOutlined } from '@ant-design/icons';
import type { ColumnsType } from 'antd/es/table';
import type { TaskItem } from '@/components/tasks/constants';
import {
  PENDING_APPROVAL_LANE,
  STATUS_LABEL,
  PendingApprovalTag,
  statusColor,
  complexityColor,
  complexityLabel,
  formatDateShort,
  isPendingApproval,
} from '@/components/tasks/constants';
import bundledApi from '@/api/bundled';
import { formatProcessText } from '@/utils/processText';
import { useResizableColumns, makeSorter } from '@/hooks/useResizableColumns';

const { Text } = Typography;

interface TasksTableViewProps {
  tasks: TaskItem[];
  loading: boolean;
  searchKeyword: string;
  workspaceId: number;
  selectedTaskId: number | null;
  /** tab 可选（063）：点待审批标记时传 'exec'，详情直达执行历史 Tab。 */
  onSelectTask: (taskId: number | null, tab?: string) => void;
  /** 列表数据被本组件修改（如批量删除成功）后，通知父组件刷新。 */
  onChanged: () => void;
}

/** 状态筛选项：all = 不筛；pending_approval 为 063 虚拟选项（按待审批数过滤而非匹配 status）。 */
const STATUS_FILTER_OPTIONS = [
  { value: 'all', label: '全部状态' },
  { value: PENDING_APPROVAL_LANE, label: '待审批' },
  { value: 'pending', label: '待执行' },
  { value: 'running', label: '进行中' },
  { value: 'success', label: '已完成' },
  { value: 'failed', label: '失败' },
];

/**
 * 构造 Table 列定义。
 *
 * 列顺序与宽度：
 *   ID(60) | 标题(flex) | 状态(100) | 待审批(100) | 复杂度(80) | 工艺(220) | 最近执行(110) | 创建时间(110)
 *
 * 标题列 ellipsis：防止长标题撑爆行宽。
 * 状态/最近执行列用 Tag：颜色与 STATUS_COLOR 一致。
 * onSelectTaskRef：待审批 Tag 点击需调父级跳转（063），经 ref 传入避免列定义随闭包重建。
 */
function buildColumns(
  onSelectTaskRef: React.MutableRefObject<(taskId: number | null, tab?: string) => void>,
): ColumnsType<TaskItem> {
  return [
    {
      title: '#',
      dataIndex: 'id',
      key: 'id',
      width: 60,
      // 054：可排序列（ID 数值排序）
      sorter: makeSorter<TaskItem>('id'),
      render: (id: number) => <Text type="secondary">#{id}</Text>,
    },
    {
      title: '标题',
      dataIndex: 'title',
      key: 'title',
      ellipsis: true,
      // 054：可排序列（标题字符串排序）
      sorter: makeSorter<TaskItem>('title'),
      render: (title: string) => <Text strong>{title}</Text>,
    },
    {
      title: '状态',
      dataIndex: 'status',
      key: 'status',
      width: 100,
      // 054：可排序列（状态枚举字符串排序）
      sorter: makeSorter<TaskItem>('status'),
      render: (status: string) => (
        <Tag color={statusColor(status)}>{STATUS_LABEL[status] ?? status}</Tag>
      ),
    },
    {
      // 063：独立「待审批」列（对齐环路列表 LoopListViewParts 模式），可排序让待审批多的任务排前。
      title: '待审批',
      dataIndex: 'pending_approval_count',
      key: 'pending_approval_count',
      width: 100,
      sorter: makeSorter<TaskItem>('pending_approval_count'),
      render: (count: number | undefined, task) =>
        isPendingApproval(task) ? (
          <PendingApprovalTag
            count={count ?? 0}
            onApprove={() => onSelectTaskRef.current(task.id, 'exec')}
          />
        ) : (
          <Text type="secondary">—</Text>
        ),
    },
    {
      title: '复杂度',
      dataIndex: 'complexity',
      key: 'complexity',
      width: 80,
      // 054：可排序列（复杂度枚举字符串排序）
      sorter: makeSorter<TaskItem>('complexity'),
      render: (complexity?: string) =>
        complexity ? (
          <Tag color={complexityColor(complexity)}>{complexityLabel(complexity)}</Tag>
        ) : (
          <Text type="secondary">—</Text>
        ),
    },
    {
      title: '工艺',
      key: 'process',
      width: 220,
      ellipsis: true,
      // 与事项/环路列表统一：#工艺id-工艺名称-工艺版本；无模板来源显示 —。
      render: (_, task) => {
        const text = formatProcessText(task.template_id, task.template_name, task.template_version);
        return text === '-' ? <Text type="secondary">—</Text> : <Tag>{text}</Tag>;
      },
    },
    {
      title: '最近执行',
      dataIndex: 'latest_execution_status',
      key: 'latest_execution_status',
      width: 110,
      render: (status?: string) =>
        status ? (
          <Tag color={statusColor(status)}>{STATUS_LABEL[status] ?? status}</Tag>
        ) : (
          <Text type="secondary">—</Text>
        ),
    },
    {
      title: '创建时间',
      dataIndex: 'created_at',
      key: 'created_at',
      width: 110,
      // 054：可排序列（ISO 时间字符串排序）
      sorter: makeSorter<TaskItem>('created_at'),
      render: (iso?: string) => <Text type="secondary">{formatDateShort(iso)}</Text>,
    },
  ];
}

/** 选中 ID 裁剪 hook：tasks 变化时移除已消失的行（与 TodoListView 同模式）。 */
function useSelectedIdsClipping(items: TaskItem[]) {
  const [selectedIds, setSelectedIds] = useState<number[]>([]);
  useEffect(() => {
    setSelectedIds(prev => {
      const alive = prev.filter(id => items.some(i => i.id === id));
      return alive.length === prev.length ? prev : alive;
    });
  }, [items]);
  return { selectedIds, setSelectedIds };
}

/**
 * 批量按钮：受 selectedIds 控制，空选时禁用。
 * 与 TodoListView BatchButton 同模式。
 */
function BatchButton({
  selectedIds,
  onBatchDelete,
}: {
  selectedIds: number[];
  onBatchDelete: (ids: number[]) => void;
}) {
  // 与 TodoListView 对齐：批量入口一直渲染，空选时只禁用不隐藏，
  // 让用户在未勾选前也能感知列表支持批量操作。
  const disabled = selectedIds.length === 0;
  const items = [
    {
      key: 'delete',
      label: '删除',
      icon: <DeleteOutlined />,
      danger: true as const,
      onClick: () => onBatchDelete(selectedIds),
      disabled,
    },
  ];
  return (
    <Dropdown menu={{ items }} trigger={['click']} disabled={disabled}>
      <Button size="small" disabled={disabled} data-testid="tasks-table-batch-trigger">
        批量 <MoreOutlined style={{ fontSize: 10 }} />
      </Button>
    </Dropdown>
  );
}

/**
 * 任务列表视图（Table）。
 *
 * 整体处理思路：
 * 1. 自带状态筛选 Select + 关键词搜索（走宿主顶栏 searchKeyword）。
 * 2. 行 onClick：选中任务，触发宿主右栏渲染详情。
 * 3. 选中行高亮：rowClassName 控制。
 * 4. 批量操作：行多选 + BatchButton（删除），与 TodoListView 操作模式一致。
 * 5. 空态：根据是否有筛选条件显示不同文案。
 */
export function TasksTableView({
  tasks,
  loading,
  searchKeyword,
  workspaceId,
  selectedTaskId,
  onSelectTask,
  onChanged,
}: TasksTableViewProps) {
  // 自带筛选态：状态。
  // 复杂度筛选在这里不加，避免 toolbar 过于拥挤；用户可切到 card 视图做复杂度筛选。
  const [statusFilter, setStatusFilter] = useState<string>('all');
  // 行选中态（批量删除用）
  const { selectedIds, setSelectedIds } = useSelectedIdsClipping(tasks);

  // 通过 AntApp.useApp() 获取 modal/message 实例，而不是使用 Modal.confirm / message 静态方法；
  // 这样批量删除确认窗才能进入当前 ConfigProvider/AntApp 上下文，亮暗主题切换时按当前主题 token 渲染。
  const { modal, message } = AntApp.useApp();

  // 过滤逻辑：状态 + 关键词（标题 OR 需求）。
  const visibleTasks = useMemo(() => {
    const kw = searchKeyword.trim().toLowerCase();
    return tasks.filter((task) => {
      // 063：「待审批」是虚拟筛选项，按待审批数过滤；真实状态仍按 status 精确匹配。
      if (statusFilter === PENDING_APPROVAL_LANE) {
        if (!isPendingApproval(task)) return false;
      } else if (statusFilter !== 'all' && task.status !== statusFilter) return false;
      if (!kw) return true;
      const titleMatch = task.title.toLowerCase().includes(kw);
      const reqMatch = (task.latest_execution_requirement ?? '').toLowerCase().includes(kw);
      return titleMatch || reqMatch;
    });
  }, [tasks, statusFilter, searchKeyword]);

  // 待审批 Tag 点击跳转需用最新 onSelectTask；用 ref 持有，列定义即可保持 useMemo 空依赖不重建。
  const onSelectTaskRef = useRef(onSelectTask);
  useEffect(() => {
    onSelectTaskRef.current = onSelectTask;
  }, [onSelectTask]);

  // 列定义：useMemo 避免每次 render 重建造成 Table 性能抖动。
  const rawColumns = useMemo(() => buildColumns(onSelectTaskRef), []);
  // 054：注入可拖拽列宽 + 受控排序 + localStorage 持久化。
  const { columns, tableProps } = useResizableColumns('tasks', rawColumns);

  // 批量删除确认：使用上下文 modal.confirm，保证弹窗随当前主题渲染；
  // 确认文案与危险按钮保持原样，避免引入额外交互变化。
  const handleBatchDelete = (ids: number[]) => {
    modal.confirm({
      title: `确认删除 ${ids.length} 个任务？`,
      content: '删除后不可恢复。',
      okText: '删除',
      okType: 'danger',
      cancelText: '取消',
      onOk: async () => {
        try {
          const result = await bundledApi.batchDeleteTasks(workspaceId, ids);
          message.success(`已删除 ${result.deleted} 个任务`);
          setSelectedIds([]);
          // 任务数据由父组件 TasksPage 持有；删除成功后必须触发父级刷新，
          // 否则表格仍显示已删除任务，用户可能误以为删除未生效。
          onChanged();
        } catch {
          message.error('删除失败');
        }
      },
    });
  };

  // 筛选 toolbar：批量按钮 + 状态 Select + 计数。
  // 批量按钮放第一位，符合用户对批量管理入口的直觉预期；padding 与事项/环路列表 toolbar 一致（6px 12px），避免三页工具栏高度不一
  const toolbar = (
    <div
      style={{
        display: 'flex',
        alignItems: 'center',
        gap: 8,
        padding: '6px 12px',
        borderBottom: '1px solid var(--color-border-light, #f0f0f0)',
        flexShrink: 0,
      }}
    >
      <BatchButton selectedIds={selectedIds} onBatchDelete={handleBatchDelete} />
      <Select
        size="small"
        value={statusFilter}
        onChange={setStatusFilter}
        options={STATUS_FILTER_OPTIONS}
        style={{ width: 120 }}
        data-testid="tasks-table-status-filter"
      />
      <Text type="secondary" style={{ fontSize: 12, marginLeft: 'auto' }}>
        已选 {selectedIds.length} 项 / 共 {visibleTasks.length} 个任务
      </Text>
    </div>
  );

  return (
    <div
      style={{ height: '100%', display: 'flex', flexDirection: 'column' }}
      data-testid="tasks-table-view"
    >
      {toolbar}
      {/* table 主体：与事项/环路列表同款配置——横向 scroll.x + 分页，
          不再用 scroll.y 固定表头（三页滚动/分页行为保持一致） */}
      <div style={{ flex: 1, minHeight: 0, overflow: 'auto' }}>
        <Table<TaskItem>
          rowKey="id"
          columns={columns}
          dataSource={visibleTasks}
          loading={loading}
          size="small"
          // 054：scroll.x 由 hook 动态计算（列宽求和），替代原硬编码 1290
          {...tableProps}
          pagination={{
            pageSize: 20,
            showSizeChanger: true,
            pageSizeOptions: ['20', '50', '100'],
            showTotal: (total) => `共 ${total} 个任务`,
          }}
          rowSelection={{
            selectedRowKeys: selectedIds,
            onChange: (keys) => setSelectedIds(keys as number[]),
          }}
          rowClassName={(record) =>
            record.id === selectedTaskId ? 'ntd-tasks-row-selected' : ''
          }
          onRow={(record) => ({
            onClick: () => onSelectTask(record.id),
            style: { cursor: 'pointer' },
          })}
          locale={{
            emptyText: (
              <Empty
                description={
                  statusFilter !== 'all' || searchKeyword.trim()
                    ? '没有符合筛选条件的任务'
                    : '暂无任务'
                }
                image={Empty.PRESENTED_IMAGE_SIMPLE}
                style={{ marginTop: 32 }}
              />
            ),
          }}
        />
      </div>
    </div>
  );
}
