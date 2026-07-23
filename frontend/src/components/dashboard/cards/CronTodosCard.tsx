// 「时间驱动 todo」卡:展示启用 cron 调度的 todo 数量与即将触发的列表。
// 数据来自 GET /api/scheduler/todos,返回的 Todo 已带后端派生的 scheduler_next_run_at。
import { Tag, Tooltip } from 'antd';
import { ClockCircleOutlined } from '@ant-design/icons';
import { getSchedulerTodos } from '@/utils/database/todos';
import { formatRelativeTime } from '@/utils/datetime';
import { useCardData } from '@/components/dashboard/useCardData';
import { useApp } from '@/hooks/useApp';
import { CardShell } from './CardShell';

// 下次触发时间渲染成相对时间(如「2 小时后」);缺失则该 todo 未真正排程。
function NextRunTag({ next }: { next: string | null | undefined }) {
  if (!next) return <Tag>未排程</Tag>;
  return (
    <Tooltip title={next}>
      <Tag color="blue">{formatRelativeTime(next)}</Tag>
    </Tooltip>
  );
}

export function CronTodosCard() {
  const { state } = useApp();
  const wsId = state.selectedWorkspace ?? 0;
  // v1 纯 workspace-scoped：scheduler todos 按 workspace 隔离
  const { data, loading, error } = useCardData(() => getSchedulerTodos(wsId), [wsId]);
  const todos = data ?? [];
  return (
    <CardShell icon={<ClockCircleOutlined />} title={`时间驱动 todo(${todos.length})`} loading={loading} error={error}>
      {todos.length === 0 ? (
        <span style={{ color: 'var(--color-text-tertiary)' }}>暂无启用 cron 的 todo</span>
      ) : (
        // 只展示前 5 条,避免卡片过长;完整列表在「事项」页的时间驱动分桶查看。
        <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
          {todos.slice(0, 5).map((t) => (
            <div key={t.id} style={{ display: 'flex', justifyContent: 'space-between', gap: 8, fontSize: 13 }}>
              <span style={{ overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{t.title}</span>
              <NextRunTag next={t.scheduler_next_run_at} />
            </div>
          ))}
        </div>
      )}
    </CardShell>
  );
}
