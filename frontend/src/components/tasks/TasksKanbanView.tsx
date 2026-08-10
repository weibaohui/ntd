// 任务看板视图：按状态横向分泳道。
// 形态参考 MemorialBoard 的 KanbanBoard：
//   5 列泳道（待审批 / pending / running / success / failed），每列卡片墙垂直排列。
//   「待审批」为 063 新增的虚拟泳道：pending_approval_count>0 的任务优先进该列，
//   让用户打开看板第一眼就看到需要人工审批的任务。
// 交互：
//   - hover 卡片有阴影增强 + translateY(-1px)（prefers-reduced-motion 关闭）
//   - cursor: pointer
//   - 点击卡片 → 调 onSelectTask 选中并切到 list 视图
//   - 点击「N 待审批」标记 → onSelectTask(id, 'exec') 直达详情执行历史（063）
// 不做拖拽（后端 PATCH /tasks/:id 未实现 status 更新，YAGNI）。

import { memo, useMemo } from 'react';
import { Empty, Skeleton, Tag, Typography } from 'antd';
import { InboxOutlined } from '@ant-design/icons';
import type { TaskItem } from '@/components/tasks/constants';
import {
  TASK_LANES,
  STATUS_LABEL,
  PendingApprovalTag,
  complexityColor,
  complexityLabel,
  formatDateShort,
  laneOfTask,
  statusColor,
} from '@/components/tasks/constants';

const { Text } = Typography;

interface TasksKanbanViewProps {
  tasks: TaskItem[];
  loading: boolean;
  workspaceId: number;
  /** tab 可选（063）：点待审批标记时传 'exec'，详情直达执行历史 Tab。 */
  onSelectTask: (taskId: number | null, tab?: string) => void;
}

/**
 * 把扁平任务列表按状态分组到各泳道。
 *
 * 处理思路：
 *   1. 以 TASK_LANES 为顺序初始化空数组。
 *   2. 遍历 tasks，按 laneOfTask（待审批优先于真实 status）推入对应泳道。
 *   3. 未匹配 status 的任务不会显示（防止脏数据塞错列）。
 */
function groupByLane(tasks: TaskItem[]): Record<string, TaskItem[]> {
  // 初始化：每个泳道一个空数组，顺序即 TASK_LANES 顺序。
  const lanes: Record<string, TaskItem[]> = {};
  for (const lane of TASK_LANES) {
    lanes[lane.status] = [];
  }
  // 分桶：laneOfTask 已处理「待审批优先」口径，未匹配泳道丢弃。
  for (const task of tasks) {
    const bucket = lanes[laneOfTask(task)];
    if (bucket) bucket.push(task);
  }
  return lanes;
}

/** 单张任务卡片。memo：task / onSelectTask 引用不变时跳过重渲染（091 性能优化）。
 *  接收 onSelectTask 而非预绑定的 onSelect，避免每张卡每渲染新建闭包破坏 memo。 */
const KanbanTaskCard = memo(function KanbanTaskCard({
  task,
  onSelectTask,
}: {
  task: TaskItem;
  onSelectTask: (id: number, tab?: string) => void;
}) {
  return (
    <div
      onClick={() => onSelectTask(task.id)}
      style={{
        // 卡片基础样式：白底、圆角、轻阴影。
        background: 'var(--color-bg-card, #fff)',
        borderRadius: 'var(--radius-md, 8px)',
        padding: 12,
        marginBottom: 8,
        // 阴影：与 TodoCard 一致的轻阴影。
        boxShadow: '0 1px 2px rgba(0, 0, 0, 0.06)',
        // 交互：cursor pointer + hover transition。
        cursor: 'pointer',
        transition: 'box-shadow 0.2s ease, transform 0.2s ease',
        // 防止内容溢出。
        overflow: 'hidden',
      }}
      // 鼠标进入：阴影增强 + 轻微上浮。
      onMouseEnter={(e) => {
        e.currentTarget.style.boxShadow = '0 4px 8px rgba(0, 0, 0, 0.1)';
        e.currentTarget.style.transform = 'translateY(-1px)';
      }}
      // 鼠标离开：恢复原状。
      onMouseLeave={(e) => {
        e.currentTarget.style.boxShadow = '0 1px 2px rgba(0, 0, 0, 0.06)';
        e.currentTarget.style.transform = 'translateY(0)';
      }}
      data-testid={`tasks-kanban-card-${task.id}`}
    >
      {/* 头部行：#id + 复杂度标签 + 类型徽标 + 待审批标记 + 创建时间 */}
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          gap: 6,
          marginBottom: 4,
        }}
      >
        <Text type="secondary" style={{ fontSize: 12 }}>
          #{task.id}
        </Text>
        {task.complexity && (
          <Tag color={complexityColor(task.complexity)} style={{ fontSize: 11, margin: 0 }}>
            {complexityLabel(task.complexity)}
          </Tag>
        )}
        {/* NTD-013：与 Table/Card 同口径，加环路/委派类型徽标。
            看板卡片未展示主状态 Tag，徽标紧跟复杂度 Tag，一眼区分任务类型。 */}
        <Text type="secondary" style={{ fontSize: 10 }}>
          {task.execution_mode === 'delegate' ? '委派' : '环路'}
        </Text>
        {/* 063：待审批标记放头部行，卡片在未展开的看板里也能一眼看到；点击直达执行历史。 */}
        <PendingApprovalTag
          count={task.pending_approval_count ?? 0}
          onApprove={() => onSelectTask(task.id, 'exec')}
        />
        <Text type="secondary" style={{ fontSize: 11, marginLeft: 'auto' }}>
          {formatDateShort(task.created_at)}
        </Text>
      </div>
      {/* 标题：原生 div + -webkit-line-clamp 实现多行省略。
          不用 antd Text：antd Typography.Text 默认 display:inline，
          与 -webkit-box 冲突会导致 line-clamp 失效、标题高度为 0。 */}
      <div
        style={{
          fontSize: 14,
          fontWeight: 600,
          lineHeight: 1.4,
          marginBottom: 6,
          // 多行省略：需要 display:-webkit-box + box-orient:vertical + line-clamp。
          display: '-webkit-box',
          WebkitBoxOrient: 'vertical',
          WebkitLineClamp: 2,
          overflow: 'hidden',
          // word-break 防止超长无空格单词撑破容器。
          wordBreak: 'break-word',
        }}
      >
        {task.title}
      </div>
      {/* NTD-013：工艺/委派信息 Tag（与 Card 同口径）：
          - 委派模式：蓝色委派信息 Tag；
          - 环路模式：若有工艺名，显示灰色工艺名 Tag（简洁，避免三段式在看板卡片过拥挤）。 */}
      {task.execution_mode === 'delegate' ? (
        (() => {
          const kindLabel = task.assignee_kind === 'expert' ? '专家' : '执行器';
          const name = task.assignee_name?.trim() || '未知处理人';
          // 文案与 Table/Card 同口径：统一「委派给：… 🚀自动接力」（NTD-013 规范轴 c.1）。
          const suffix = task.auto_continue ? ' 🚀自动接力' : '';
          return (
            <Tag color="blue" style={{ fontSize: 11, margin: '0 0 6px 0' }}>
              {`委派给：${name}（${kindLabel}）${suffix}`}
            </Tag>
          );
        })()
      ) : (
        task.template_name && (
          <Tag style={{ fontSize: 11, margin: '0 0 6px 0' }}>{task.template_name}</Tag>
        )
      )}
      {/* 最近执行状态 */}
      {task.latest_execution_status && (
        <div style={{ display: 'flex', gap: 4, flexWrap: 'wrap' }}>
          <Tag color={statusColor(task.latest_execution_status)} style={{ fontSize: 11, margin: 0 }}>
            {STATUS_LABEL[task.latest_execution_status] ?? task.latest_execution_status}
          </Tag>
        </div>
      )}
    </div>
  );
});

/** 单个泳道列。memo：items / onSelectTask 引用不变时跳过重渲染（091 性能优化）。 */
const KanbanLane = memo(function KanbanLane({
  label,
  color,
  items,
  onSelectTask,
}: {
  status: string;
  label: string;
  color: string;
  items: TaskItem[];
  onSelectTask: (id: number, tab?: string) => void;
}) {
  return (
    <div
      style={{
        // 4 列等宽：flex:1 + minWidth 220 防止卡片文字被压扁。
        flex: 1,
        minWidth: 220,
        // 列内垂直布局：头部 + 卡片列表。
        display: 'flex',
        flexDirection: 'column',
        // 列间分隔：右边框轻灰。
        borderRight: '1px solid var(--color-border-light, #f0f0f0)',
        // 最后一列去掉右边框（视觉对称）。
        // 实际由 :last-child 处理，这里 inline 不便写。
      }}
    >
      {/* 列头：圆点 + 标签 + 计数 */}
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          gap: 8,
          padding: '12px 16px',
          borderBottom: '1px solid var(--color-border-light, #f0f0f0)',
        }}
      >
        <span
          style={{
            display: 'inline-block',
            width: 8,
            height: 8,
            borderRadius: '50%',
            background: color,
          }}
        />
        <Text strong style={{ fontSize: 13 }}>
          {label}
        </Text>
        <Text type="secondary" style={{ fontSize: 12, marginLeft: 'auto' }}>
          {items.length}
        </Text>
      </div>
      {/* 卡片列表区：可滚动 */}
      <div
        style={{
          flex: 1,
          padding: 12,
          overflowY: 'auto',
          minHeight: 100,
        }}
      >
        {items.length === 0 ? (
          // 空泳道：灰色 Inbox 图标占位。
          <div
            style={{
              display: 'flex',
              flexDirection: 'column',
              alignItems: 'center',
              justifyContent: 'center',
              padding: 32,
              color: 'var(--color-text-quaternary, #bfbfbf)',
            }}
          >
            <InboxOutlined style={{ fontSize: 24, marginBottom: 8 }} />
            <Text type="secondary" style={{ fontSize: 12 }}>
              暂无{label}任务
            </Text>
          </div>
        ) : (
          items.map((task) => (
            <KanbanTaskCard
              key={task.id}
              task={task}
              onSelectTask={onSelectTask}
            />
          ))
        )}
      </div>
    </div>
  );
});

export function TasksKanbanView({
  tasks,
  loading,
  onSelectTask,
}: TasksKanbanViewProps) {
  // 把扁平任务列表按状态分到各泳道（待审批优先，063）。
  // useMemo：tasks 不变时不重新分组。
  const lanes = useMemo(() => groupByLane(tasks), [tasks]);

  // loading 态：按泳道数渲染骨架屏。
  if (loading) {
    return (
      <div
        style={{
          display: 'flex',
          height: '100%',
          gap: 0,
        }}
      >
        {TASK_LANES.map((lane) => (
          <div
            key={lane.status}
            style={{
              flex: 1,
              minWidth: 220,
              padding: 12,
              borderRight: '1px solid var(--color-border-light, #f0f0f0)',
            }}
          >
            <Skeleton active paragraph={{ rows: 4 }} />
          </div>
        ))}
      </div>
    );
  }

  // 全空态：整个看板无任务时显示空态。
  if (tasks.length === 0) {
    return (
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center',
          height: '100%',
        }}
      >
        <Empty description="暂无任务" image={Empty.PRESENTED_IMAGE_SIMPLE} />
      </div>
    );
  }

  return (
    <div
      style={{
        // 整个看板容器：横向 4 列 + 高度撑满父容器。
        display: 'flex',
        height: '100%',
        overflow: 'hidden',
      }}
      data-testid="tasks-kanban-board"
    >
      {TASK_LANES.map((lane) => (
        <KanbanLane
          key={lane.status}
          status={lane.status}
          label={lane.label}
          color={lane.color}
          items={lanes[lane.status]}
          onSelectTask={onSelectTask}
        />
      ))}
    </div>
  );
}
