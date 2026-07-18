// 仪表盘「最近执行记录」表格卡。
//
// 从 Dashboard.tsx 抽离:原页面把这张表硬编码在 Masonry 瀑布流之外单独渲染,
// Tab 化后它归入「总览」Tab。独立成组件便于复用,并把列定义收敛到一处,
// 同时把每列的渲染逻辑拆成单一职责的小函数,降低主组件复杂度。
import type { ColumnsType } from 'antd/es/table';
import { Card, Table, Badge, Tag, Empty } from 'antd';
import { ThunderboltOutlined } from '@ant-design/icons';
import type { DashboardStats, Todo } from '@/types';
import { getExecutorOption } from '@/types';
import { formatRelativeTime } from '@/utils/datetime';

// 单条执行记录的类型(后端按 started_at 倒序返回最多 10 条)。
type RecentExecution = DashboardStats['recent_executions'][number];

interface RecentExecutionsTableProps {
  executions: RecentExecution[];
  /** 全量 todo,用于把 todo_id 反查为标题;查不到时回退「任务 #id」。 */
  todos: Todo[];
}

// 把 todo_id 渲染为可读标题:优先用 todo 标题,缺失才回退编号,避免空白单元格。
function renderTodoTitle(todoId: number, todos: Todo[]) {
  const todo = todos.find((t) => t.id === todoId);
  return <span style={{ fontWeight: 600 }}>{todo?.title ?? `任务 #${todoId}`}</span>;
}

// 执行器列:未指定执行器显示「-」,否则用全局配色 Tag 强化辨识。
function renderExecutor(executor: string | null) {
  if (!executor) return <span>-</span>;
  const opt = getExecutorOption(executor);
  return (
    <Tag color={opt.color} style={{ fontWeight: 600 }}>
      {opt.label}
    </Tag>
  );
}

// 触发类型列:仅区分 cron(时间驱动)与手动,配色与 TriggerSourceCard 保持一致。
function renderTrigger(triggerType: string) {
  return (
    <Tag color={triggerType === 'cron' ? '#8b5cf6' : '#6b7280'} style={{ fontSize: 10 }}>
      {triggerType === 'cron' ? 'Cron' : '手动'}
    </Tag>
  );
}

// 状态列:success/failed/running 三态,用 Badge 颜色 + 中文文案表达。
function renderStatus(status: string) {
  const isSuccess = status === 'success';
  const isFailed = status === 'failed';
  return (
    <Badge
      status={isSuccess ? 'success' : isFailed ? 'error' : 'processing'}
      text={isSuccess ? '成功' : isFailed ? '失败' : '运行中'}
    />
  );
}

// 时间列:相对时间(如「3 分钟前」),弱化颜色让用户聚焦状态与执行器列。
function renderTime(startedAt: string) {
  return <span style={{ fontSize: 12, color: 'var(--color-text-tertiary)' }}>{formatRelativeTime(startedAt)}</span>;
}

// 列定义抽成纯函数:依赖 todos(标题反查)。
// 标注 ColumnsType<RecentExecution> 让 render 的 value 形参得到精确类型而非隐式 any。
function buildColumns(todos: Todo[]): ColumnsType<RecentExecution> {
  return [
    { title: '任务', dataIndex: 'todo_id', key: 'todo_id', render: (v) => renderTodoTitle(v as number, todos) },
    { title: '执行器', dataIndex: 'executor', key: 'executor', width: 100, render: (v) => renderExecutor(v as string | null) },
    { title: '触发', dataIndex: 'trigger_type', key: 'trigger_type', width: 70, render: (v) => renderTrigger(v as string) },
    { title: '状态', dataIndex: 'status', key: 'status', width: 90, render: (v) => renderStatus(v as string) },
    { title: '时间', dataIndex: 'started_at', key: 'started_at', width: 140, render: (v) => renderTime(v as string) },
  ];
}

export function RecentExecutionsTable({ executions, todos }: RecentExecutionsTableProps) {
  const columns = buildColumns(todos);
  return (
    <Card
      title={
        <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
          <ThunderboltOutlined />
          <span>最近执行记录</span>
        </div>
      }
      style={{ borderRadius: 12 }}
      styles={{ body: { padding: '16px 20px' } }}
    >
      {executions.length > 0 ? (
        <Table
          columns={columns}
          dataSource={executions}
          rowKey="id"
          pagination={false}
          size="small"
          scroll={{ x: 'max-content' }}
        />
      ) : (
        <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description="暂无执行记录" />
      )}
    </Card>
  );
}
