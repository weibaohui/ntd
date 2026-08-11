// 任务卡片视图：卡片墙网格。
// 形态参考 TodoCenterCardView：
//   响应式 grid（minmax(320px, 1fr)），
//   每张卡片承载任务摘要 + 「再次执行」次要操作。
// 交互：
//   - hover 卡片有阴影增强 + translateY(-1px)（prefers-reduced-motion 关闭）
//   - cursor: pointer
//   - 点击卡片 → 调 onSelectTask 选中并切到 list 视图
// 自带筛选：
//   - 状态 Select（全部 / 待执行 / 进行中 / 已完成 / 失败）
//   - 复杂度 Select（全部 / 轻量 / 标准 / 复杂）
//   - 关键词搜索走宿主顶栏 searchKeyword

import { useMemo, useState } from 'react';
import { Tag, Typography, Empty, Select, Spin, message, Modal, Input, Button } from 'antd';
import { ThunderboltOutlined } from '@ant-design/icons';
import bundledApi from '@/api/bundled';
import type { TaskItem } from '@/components/tasks/constants';
import {
  STATUS_LABEL,
  PendingApprovalTag,
  TASK_STATUS_FILTER_OPTIONS,
  statusColor,
  complexityColor,
  complexityLabel,
  formatDateShort,
  matchesTaskStatusFilter,
} from '@/components/tasks/constants';
// NTD-013 / CodeRabbit Opinion 2：执行方式徽标 + 工艺/委派信息 Tag 收口到共享组件，
// 三视图共用，杜绝委派文案再次漂移（详见 TaskExecutionTags.tsx）。
import { ExecutionModeBadge, TaskExecutionInfoTag } from '@/components/tasks/TaskExecutionTags';

const { Text, Paragraph } = Typography;

interface TasksCardViewProps {
  tasks: TaskItem[];
  loading: boolean;
  searchKeyword: string;
  workspaceId: number;
  /** tab 可选（063）：点待审批标记时传 'exec'，详情直达执行历史 Tab。 */
  onSelectTask: (taskId: number | null, tab?: string) => void;
}

/** 复杂度筛选项。 */
const COMPLEXITY_FILTER_OPTIONS = [
  { value: 'all', label: '全部复杂度' },
  { value: 'light', label: '轻量' },
  { value: 'standard', label: '标准' },
  { value: 'complex', label: '复杂' },
];

/**
 * 单张任务卡片。
 *
 * 设计原则（与 TodoCenterCard 一致）：
 *   - 卡片只放一个主操作（点击进详情）
 *   - 次要操作（再次执行）放在卡片底部，用 Modal 二次确认
 */
function TaskCard({
  task,
  workspaceId,
  onSelect,
  onApprove,
  onTriggered,
}: {
  task: TaskItem;
  workspaceId: number;
  onSelect: () => void;
  /** 063：点待审批标记直达详情执行历史 Tab（与卡片本体点进概览区分开）。 */
  onApprove: () => void;
  onTriggered: () => void;
}) {
  // 卡片主体点击 → 选中任务。「再次执行」的交互（Modal/输入/提交）已拆到 TaskReexecButton，
  // 使 TaskCard 回到 ≤150 行（NTD-013 / CodeRabbit Opinion 2）；workspaceId/onTriggered 仅转发给它。
  const handleCardClick = () => onSelect();

  return (
    <div
      onClick={handleCardClick}
      data-testid={`tasks-card-${task.id}`}
      style={{
        // 卡片基础样式：白底、圆角、轻阴影。
        background: 'var(--color-bg-card, #fff)',
        borderRadius: 'var(--radius-md, 8px)',
        padding: 14,
        // 阴影：与 原 TodoCard 一致的轻阴影。
        boxShadow: '0 1px 2px rgba(0, 0, 0, 0.06)',
        // 交互：cursor pointer + hover transition。
        cursor: 'pointer',
        transition: 'box-shadow 0.2s ease, transform 0.2s ease',
        // 防止内容溢出。
        overflow: 'hidden',
        // 卡片内用 flex column 布局。
        display: 'flex',
        flexDirection: 'column',
        gap: 8,
        minHeight: 140,
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
    >
      {/* 头部行：#id + 状态 Tag + 类型徽标 + 待审批标记 + 创建时间 */}
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          gap: 6,
        }}
      >
        <Text type="secondary" style={{ fontSize: 12 }}>
          #{task.id}
        </Text>
        <Tag color={statusColor(task.status)} style={{ fontSize: 11, margin: 0 }}>
          {STATUS_LABEL[task.status] ?? task.status}
        </Tag>
        {/* NTD-013：与 Table 视图同口径，状态 Tag 旁加环路/委派类型徽标（共享组件）。 */}
        <ExecutionModeBadge mode={task.execution_mode} />
        {/* 063：待审批标记与状态 Tag 同行展示，不进详情即可感知。 */}
        <PendingApprovalTag count={task.pending_approval_count ?? 0} onApprove={onApprove} />
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
          marginBottom: 8,
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

      {/* 需求摘要：最多 2 行省略，无则不渲染 */}
      {task.latest_execution_requirement && (
        <Paragraph
          type="secondary"
          style={{ fontSize: 12, margin: 0 }}
          ellipsis={{ rows: 2 }}
        >
          {task.latest_execution_requirement}
        </Paragraph>
      )}

      {/* 底部行：复杂度 Tag + 工艺/委派 Tag + 再次执行按钮 */}
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          gap: 4,
          marginTop: 'auto',
          flexWrap: 'wrap',
        }}
      >
        {task.complexity && (
          <Tag color={complexityColor(task.complexity)} style={{ fontSize: 11, margin: 0 }}>
            {complexityLabel(task.complexity)}
          </Tag>
        )}
        {/* NTD-013：委派信息 Tag 走共享组件（防文案漂移）；环路分支卡片只显示工艺名（避免拥挤）。
            delegateStyle 控制委派 Tag 在卡片底行的字号/外边距。 */}
        <TaskExecutionInfoTag
          task={task}
          delegateStyle={{ fontSize: 11, margin: 0 }}
          loopTag={
            task.template_name ? (
              <Tag style={{ fontSize: 11, margin: 0 }}>{task.template_name}</Tag>
            ) : undefined
          }
        />
        <TaskReexecButton task={task} workspaceId={workspaceId} onTriggered={onTriggered} />
      </div>
    </div>
  );
}

/**
 * 「再次执行」按钮 + Modal：从 TaskCard 拆出（NTD-013 / CodeRabbit Opinion 2，降行数）。
 *
 * 自带 Modal 开关 / 输入态 / 提交逻辑，调用方只关心创建成功后的 onTriggered 回调。
 * 按钮点击 stopPropagation 防止冒泡到卡片本体（否则会同时触发选中跳详情）。
 * Modal 经 antd portal 渲染到 body，故即便作为底部 flex 行的子节点也不影响卡片布局。
 */
function TaskReexecButton({
  task,
  workspaceId,
  onTriggered,
}: {
  task: TaskItem;
  workspaceId: number;
  onTriggered: () => void;
}) {
  // Modal 开关 + 输入态 + 提交中态，全部局部受控，不污染 TaskCard。
  const [reexecOpen, setReexecOpen] = useState(false);
  const [newReq, setNewReq] = useState('');
  const [busy, setBusy] = useState(false);

  const handleClick = (e: React.MouseEvent) => {
    // 阻止冒泡到卡片 onClick，避免点「再次执行」同时触发选中跳详情。
    e.stopPropagation();
    // 初始值优先用 description（更贴近真实诉求），缺省回退 title。
    setNewReq(task.description || task.title);
    setReexecOpen(true);
  };

  const handleSubmit = async () => {
    // 空需求兜底：拒绝提交并提示，不发空请求。
    if (!newReq.trim()) {
      message.warning('请输入需求');
      return;
    }
    setBusy(true);
    try {
      await bundledApi.createTaskExecution(workspaceId, task.id, newReq);
      message.success('新执行已创建');
      setReexecOpen(false);
      onTriggered();
    } catch (err) {
      // 错误就地提示，不向上抛（Modal 保持打开，用户可改后重试）。
      message.error(`创建失败：${err instanceof Error ? err.message : String(err)}`);
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <Button
        type="text"
        size="small"
        icon={<ThunderboltOutlined />}
        onClick={handleClick}
        style={{
          marginLeft: 'auto',
          fontSize: 12,
          color: 'var(--color-primary, #1677ff)',
        }}
      >
        再次执行
      </Button>
      {/* 再次执行 Modal：局部受控，portal 渲染。 */}
      <Modal
        title="输入这次的需求"
        open={reexecOpen}
        onCancel={() => setReexecOpen(false)}
        onOk={handleSubmit}
        confirmLoading={busy}
        okText="开始执行"
      >
        <Input.TextArea value={newReq} onChange={(e) => setNewReq(e.target.value)} rows={4} />
      </Modal>
    </>
  );
}

/**
 * 任务卡片视图。
 *
 * 整体处理思路：
 * 1. 自带筛选器（状态 Select + 复杂度 Select），与宿主顶栏 searchKeyword 联动。
 * 2. 响应式 grid 布局：minmax(320px, 1fr) 自适应列数。
 * 3. 空态：根据是否有筛选条件显示不同文案。
 */
export function TasksCardView({
  tasks,
  loading,
  searchKeyword,
  workspaceId,
  onSelectTask,
}: TasksCardViewProps) {
  // 自带筛选态：状态 + 复杂度。
  const [statusFilter, setStatusFilter] = useState<string>('all');
  const [complexityFilter, setComplexityFilter] = useState<string>('all');

  // 过滤逻辑：
  //   1. 状态筛选（all = 不筛）
  //   2. 复杂度筛选（all = 不筛）
  //   3. 关键词搜索（标题 includes OR 需求 includes）
  // useMemo：依赖 tasks/三个筛选条件，任一变化重算。
  const visibleTasks = useMemo(() => {
    const kw = searchKeyword.trim().toLowerCase();
    return tasks.filter((task) => {
      // 状态过滤走共享谓词（constants.tsx 唯一事实源，含 063「待审批」虚拟项口径）。
      if (!matchesTaskStatusFilter(task, statusFilter)) return false;
      // 复杂度过滤：all 跳过；未设置 complexity 的任务被过滤掉。
      if (complexityFilter !== 'all' && task.complexity !== complexityFilter) return false;
      // 关键词过滤：空跳过；匹配 title 或 latest_execution_requirement。
      if (!kw) return true;
      const titleMatch = task.title.toLowerCase().includes(kw);
      const reqMatch = (task.latest_execution_requirement ?? '').toLowerCase().includes(kw);
      return titleMatch || reqMatch;
    });
  }, [tasks, statusFilter, complexityFilter, searchKeyword]);

  // 筛选工具条：状态 Select + 复杂度 Select。
  // 与 TodoCenterCardView 的 toolbar 风格一致。
  const toolbar = (
    <div
      style={{
        display: 'flex',
        gap: 8,
        padding: '12px 16px',
        borderBottom: '1px solid var(--color-border-light, #f0f0f0)',
        flexWrap: 'wrap',
      }}
    >
      <Select
        size="small"
        value={statusFilter}
        onChange={setStatusFilter}
        options={TASK_STATUS_FILTER_OPTIONS}
        style={{ width: 120 }}
        data-testid="tasks-card-status-filter"
      />
      <Select
        size="small"
        value={complexityFilter}
        onChange={setComplexityFilter}
        options={COMPLEXITY_FILTER_OPTIONS}
        style={{ width: 140 }}
        data-testid="tasks-card-complexity-filter"
      />
      <Text type="secondary" style={{ fontSize: 12, marginLeft: 'auto', alignSelf: 'center' }}>
        共 {visibleTasks.length} 个任务
      </Text>
    </div>
  );

  // loading 态：Spin 覆盖整页。
  if (loading) {
    return (
      <div style={{ padding: 16, height: '100%', overflow: 'auto' }}>
        {toolbar}
        <div style={{ padding: 32, textAlign: 'center' }}>
          <Spin />
        </div>
      </div>
    );
  }

  // 全空态：根据是否有筛选条件显示不同文案。
  if (visibleTasks.length === 0) {
    const hasFilter =
      statusFilter !== 'all' || complexityFilter !== 'all' || searchKeyword.trim() !== '';
    return (
      <div style={{ padding: 16, height: '100%', overflow: 'auto' }}>
        {toolbar}
        <Empty
          description={hasFilter ? '没有符合筛选条件的任务' : '暂无任务'}
          style={{ marginTop: 48 }}
        />
      </div>
    );
  }

  return (
    <div
      style={{
        padding: 16,
        height: '100%',
        overflow: 'auto',
      }}
      data-testid="tasks-card-view"
    >
      {toolbar}
      {/* 响应式卡片墙：auto-fill + minmax(320px, 1fr) 自适应列数 */}
      <div
        style={{
          display: 'grid',
          gridTemplateColumns: 'repeat(auto-fill, minmax(320px, 1fr))',
          gap: 16,
          marginTop: 16,
        }}
      >
        {visibleTasks.map((task) => (
          <TaskCard
            key={task.id}
            task={task}
            workspaceId={workspaceId}
            onSelect={() => onSelectTask(task.id)}
            onApprove={() => onSelectTask(task.id, 'exec')}
            onTriggered={() => onSelectTask(task.id)}
          />
        ))}
      </div>
    </div>
  );
}
