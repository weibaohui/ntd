// 任务列表视图：AntD Table 单表格 + 自带筛选 toolbar。
// 形态参考 ItemsPage 列表态的双栏联动：
//   左侧 Table 行可点击选中任务，触发宿主右栏渲染 TaskDetailPanel。

import { useEffect, useMemo, useState } from 'react';
import { Button, Dropdown, Modal, Table, Tag, Typography, Select, Empty, message } from 'antd';
import { MoreOutlined, DeleteOutlined } from '@ant-design/icons';
import type { ColumnsType } from 'antd/es/table';
import type { TaskItem } from '@/components/tasks/constants';
import {
  STATUS_LABEL,
  statusColor,
  complexityColor,
  complexityLabel,
  formatDateShort,
} from '@/components/tasks/constants';
import bundledApi from '@/api/bundled';

const { Text } = Typography;

interface TasksTableViewProps {
  tasks: TaskItem[];
  loading: boolean;
  searchKeyword: string;
  workspaceId: number;
  selectedTaskId: number | null;
  onSelectTask: (taskId: number | null) => void;
}

/** 状态筛选项：all = 不筛。 */
const STATUS_FILTER_OPTIONS = [
  { value: 'all', label: '全部状态' },
  { value: 'pending', label: '待执行' },
  { value: 'running', label: '进行中' },
  { value: 'success', label: '已完成' },
  { value: 'failed', label: '失败' },
];

/**
 * 构造 Table 列定义。
 *
 * 列顺序与宽度：
 *   ID(60) | 标题(flex) | 状态(100) | 复杂度(80) | 模板(120) | 最近执行(110) | 创建时间(110)
 *
 * 标题列 ellipsis：防止长标题撑爆行宽。
 * 状态/最近执行列用 Tag：颜色与 STATUS_COLOR 一致。
 */
function buildColumns(): ColumnsType<TaskItem> {
  return [
    {
      title: '#',
      dataIndex: 'id',
      key: 'id',
      width: 60,
      render: (id: number) => <Text type="secondary">#{id}</Text>,
    },
    {
      title: '标题',
      dataIndex: 'title',
      key: 'title',
      ellipsis: true,
      render: (title: string) => <Text strong>{title}</Text>,
    },
    {
      title: '状态',
      dataIndex: 'status',
      key: 'status',
      width: 100,
      render: (status: string) => (
        <Tag color={statusColor(status)}>{STATUS_LABEL[status] ?? status}</Tag>
      ),
    },
    {
      title: '复杂度',
      dataIndex: 'complexity',
      key: 'complexity',
      width: 80,
      render: (complexity?: string) =>
        complexity ? (
          <Tag color={complexityColor(complexity)}>{complexityLabel(complexity)}</Tag>
        ) : (
          <Text type="secondary">—</Text>
        ),
    },
    {
      title: '工艺模板',
      dataIndex: 'template_name',
      key: 'template_name',
      width: 130,
      ellipsis: true,
      render: (name?: string) =>
        name ? <Tag>{name}</Tag> : <Text type="secondary">—</Text>,
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
  if (selectedIds.length === 0) return null;
  const items = [
    {
      key: 'delete',
      label: '删除',
      icon: <DeleteOutlined />,
      danger: true as const,
      onClick: () => onBatchDelete(selectedIds),
    },
  ];
  return (
    <Dropdown menu={{ items }} trigger={['click']}>
      <Button size="small" data-testid="tasks-table-batch-trigger">
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
}: TasksTableViewProps) {
  // 自带筛选态：状态。
  // 复杂度筛选在这里不加，避免 toolbar 过于拥挤；用户可切到 card 视图做复杂度筛选。
  const [statusFilter, setStatusFilter] = useState<string>('all');
  // 行选中态（批量删除用）
  const { selectedIds, setSelectedIds } = useSelectedIdsClipping(tasks);

  // 过滤逻辑：状态 + 关键词（标题 OR 需求）。
  const visibleTasks = useMemo(() => {
    const kw = searchKeyword.trim().toLowerCase();
    return tasks.filter((task) => {
      if (statusFilter !== 'all' && task.status !== statusFilter) return false;
      if (!kw) return true;
      const titleMatch = task.title.toLowerCase().includes(kw);
      const reqMatch = (task.latest_execution_requirement ?? '').toLowerCase().includes(kw);
      return titleMatch || reqMatch;
    });
  }, [tasks, statusFilter, searchKeyword]);

  // 列定义：useMemo 避免每次 render 重建造成 Table 性能抖动。
  const columns = useMemo(() => buildColumns(), []);

  // 批量删除确认
  const handleBatchDelete = (ids: number[]) => {
    Modal.confirm({
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
        } catch {
          message.error('删除失败');
        }
      },
    });
  };

  // 筛选 toolbar：状态 Select + 批量按钮 + 计数。
  const toolbar = (
    <div
      style={{
        display: 'flex',
        alignItems: 'center',
        gap: 8,
        padding: '8px 12px',
        borderBottom: '1px solid var(--color-border-light, #f0f0f0)',
      }}
    >
      <Select
        size="small"
        value={statusFilter}
        onChange={setStatusFilter}
        options={STATUS_FILTER_OPTIONS}
        style={{ width: 120 }}
        data-testid="tasks-table-status-filter"
      />
      <BatchButton selectedIds={selectedIds} onBatchDelete={handleBatchDelete} />
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
      <div style={{ flex: 1, overflow: 'auto' }}>
        <Table<TaskItem>
          rowKey="id"
          columns={columns}
          dataSource={visibleTasks}
          loading={loading}
          size="small"
          pagination={false}
          scroll={{ y: 'calc(100vh - 240px)' }}
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
