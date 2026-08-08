// 嵌入式任务详情面板（合并环路详情版）。
// 任务与环路是 1:1 关系（task.loop_id → loop.id），本面板合并两个详情页：
// - Tab 1 概览：任务描述 + 环路基本信息（工作空间/待审批）+ 全局限制 + 最新执行进度
// - Tab 2 执行环路：来源工艺面包屑 + SVG DAG 流程图（复用 LoopFlowGraph）
// - Tab 3 执行历史：分页执行列表 + TokenSummaryBar + StepExecList + BlackboardDrawer
//
// 子组件（Tab 内容）拆分至 TaskDetailTabs.tsx 以控制文件大小。

import { useEffect, useState, useCallback } from 'react';
import {
  Tabs, Tag, Button, Typography, Spin, Space, Badge,
  message, Modal, Input, Empty, Popconfirm,
} from 'antd';
import {
  ThunderboltOutlined, DeleteOutlined,
} from '@ant-design/icons';
import bundledApi from '@/api/bundled';
import * as dbLoops from '@/utils/database/loops';
import { useProjectDirectories } from '@/utils/workspaceDisplay';
import { useViewState } from '@/hooks/useViewState';
import type { LoopDetail } from '@/types/loop';
import { complexityColor, complexityLabel, statusColor } from './constants';
import {
  OverviewTab, DAGTab, ExecHistoryTab,
} from './TaskDetailTabs';
import type { StepInfo } from './TaskDetailTabs';
import { DiscussionTab } from './discussion/DiscussionTab';
import styles from './TaskDetailPanel.module.css';

const { Text } = Typography;

// Tabs key 白名单：?tab= query 的合法值，非法值一律回退到「概览」。
// 帖子页返回本任务-讨论 tab 时，URL 带 ?tab=discussion，Tabs 据此恢复选中态。
const TAB_KEYS = ['overview', 'dag', 'exec', 'discussion'] as const;

// 需求 092 P2：自动接力的轮数硬上限。必须与后端 MAX_DELEGATE_ROUNDS（completion 接力护栏）
// 保持一致——前端只读 continue_rounds，不做护栏决策，仅据此展示进度与「已达上限」状态。
// 集中为常量而非魔法数，便于双端口径变更时一处定位。
const MAX_DELEGATE_ROUNDS = 10;

// ====== 类型定义 ======

interface TaskDetailPanelProps {
  taskId: number;
  workspaceId: number;
  /** 再次执行成功后回调，让宿主重拉列表。 */
  onTriggered?: () => void;
  /** 任务标题加载完成后回调，供外层 PageCard 动态更新标题。 */
  onTitleReady?: (title: string) => void;
  /** 点击 DAG 节点上的事项标题跳转事项详情。 */
  onOpenTodo?: (todoId: number) => void;
  /** 环路状态变更（启停/删除）后通知宿主刷新列表。 */
  onLoopChanged?: () => void;
}

interface ExecInfo {
  id: number;
  status: string;
  started_at?: string;
  finished_at?: string;
  total_steps: number;
  completed_steps: number;
  failed_steps: number;
  requirement?: string;
  pending_approval_count?: number;
}

interface TaskDetailData {
  task: { id: number; title: string; status: string; description?: string; workspace_id?: number; loop_id?: number; execution_mode?: string; assignee_kind?: string; assignee_name?: string; auto_continue?: boolean; continue_rounds?: number };
  template?: { display_name?: string; version?: string; complexity?: string };
  steps: StepInfo[];
  executions: ExecInfo[];
  loop?: { id: number; workspace_id?: number };
}

// ====== 子组件 ======

/** 是否为「管家自动接力」任务：委派 + 开启自动接力 + 专家处理人（执行器 P1 已禁用接力）。 */
function isAutoRelayTask(task: TaskDetailData['task']): boolean {
  return task.execution_mode === 'delegate'
    && !!task.auto_continue
    && task.assignee_kind === 'expert';
}

/** 委派任务处理人展示文案：「专家/执行器 名称」；缺名回退「—」。 */
function assigneeLabel(task: TaskDetailData['task']): string {
  const kind = task.assignee_kind === 'expert' ? '专家'
    : task.assignee_kind === 'executor' ? '执行器' : '处理人';
  return task.assignee_name ? `${kind} ${task.assignee_name}` : '—';
}

/**
 * 自动接力进度徽标（需求 092 P2）。仅管家接力任务显示，文案「管家调度中 N/MAX」：
 * - 未达上限用 processing 蓝，直观表达「进行中」；
 * - 达上限用 warning 橙，与后端 HitLimit 写的「已达上限」说明帖呼应，提示用户接管。
 * 非接力任务（环路 / 手动单跑 / 执行器委派）返回 null，标题行不留空位。
 */
function RelayBadge({ task }: { task: TaskDetailData['task'] }) {
  if (!isAutoRelayTask(task)) return null;
  const rounds = task.continue_rounds ?? 0;
  // 严格 >：与后端 plan_delegate_relay 的 `rounds > max` 护栏口径一致（设计 §5.2）。
  // 计数在判定前已 +1，故达上限时 DB 里是 11（第 11 跳被熔断）——此时才转橙；
  // 若用 >= 会在 rounds=10（仍被允许推进的最后一跳）提前变橙，与后端语义不符。
  const atLimit = rounds > MAX_DELEGATE_ROUNDS;
  return (
    <Tag color={atLimit ? 'warning' : 'processing'}>
      管家调度中 {rounds}/{MAX_DELEGATE_ROUNDS}
    </Tag>
  );
}

/** 顶部条：标题 + 状态/复杂度 + 元信息 + 删除 + 再次执行。 */
function DetailHeader({
  task, template, loopDetail, onExecute, onDelete,
}: {
  task: TaskDetailData['task'];
  template?: TaskDetailData['template'];
  loopDetail: LoopDetail | null;
  onExecute: () => void;
  onDelete: () => void;
}) {
  return (
    <div className={styles.headerBar}>
      <div className={styles.headerMain}>
        <div className={styles.titleRow}>
          <Text type="secondary">#{task.id}</Text>
          <h2 className={styles.taskTitle}>{task.title}</h2>
          <Tag color={statusColor(task.status)}>{task.status}</Tag>
          {template?.complexity && (
            <Tag color={complexityColor(template.complexity)}>{complexityLabel(template.complexity)}</Tag>
          )}
          {/* 管家自动接力任务的进度徽标（委派 + 自动接力 + 专家）。非此类任务返回 null。 */}
          <RelayBadge task={task} />
        </div>
        <div className={styles.metaRow}>
          {task.execution_mode === 'delegate' ? (
            // 委派任务无工艺/版本概念，改展示处理人（专家/执行器 + 名称）。
            <>
              <span>处理人：{assigneeLabel(task)}</span>
            </>
          ) : (
            <>
              <span>工艺：{template?.display_name ?? '—'}</span>
              <span className={styles.metaDivider}>·</span>
              <span>版本：{template?.version ?? '—'}</span>
            </>
          )}
        </div>
      </div>
      <Space>
        {loopDetail && (
          <Popconfirm title="确定删除此环路？" onConfirm={onDelete} okText="删除" cancelText="取消">
            <Button icon={<DeleteOutlined />} danger size="small">删除</Button>
          </Popconfirm>
        )}
        {/* 委派任务无环路执行概念，「再次执行」走 loop 路径不适用；用户在讨论区 @ 继续推进。 */}
        {task.execution_mode !== 'delegate' ? (
          <Button icon={<ThunderboltOutlined />} type="primary" onClick={onExecute}>再次执行</Button>
        ) : null}
      </Space>
    </div>
  );
}

// ====== 主组件 ======

/**
 * 任务详情面板（合并环路详情版）。
 * 数据获取：先拉任务详情 → 有 loop_id 则并行拉完整 LoopDetail。
 * Tab 结构：概览 / 执行环路(DAG) / 执行历史。
 */
export function TaskDetailPanel({
  taskId, workspaceId, onTriggered, onTitleReady,
  onOpenTodo, onLoopChanged,
}: TaskDetailPanelProps) {
  const [loading, setLoading] = useState(false);
  const [detail, setDetail] = useState<TaskDetailData | null>(null);
  const [loopDetail, setLoopDetail] = useState<LoopDetail | null>(null);
  const [loopLoading, setLoopLoading] = useState(false);
  const [triggering, setTriggering] = useState(false);
  const [reqModalOpen, setReqModalOpen] = useState(false);
  const [newRequirement, setNewRequirement] = useState('');
  // 讨论区 running 帖数量（DiscussionTab 上报），用于「讨论」Tab 角标（M4）。
  // 必须在下方所有 early return 之前声明：首渲染 loading 提前 return 时 hooks 也要执行，
  // 否则二次渲染多一个 hook → React「Rendered more hooks than during the previous render」崩溃。
  const [discussionRunning, setDiscussionRunning] = useState(0);
  const { dirs: projectDirs } = useProjectDirectories();
  // URL ?tab= 驱动 Tabs 选中态（对齐 Settings 页模式）：帖子页返回任务-讨论 tab 时
  // 返回 URL 带 ?tab=discussion，此处解析出 activeTab 落到对应 Tab；非法值回退「概览」。
  // 切 tab 用 replaceUrl：只更新 URL 不压 history，避免浏览器后退逐个回退 tab 而非离开页面。
  const { activeTab, replaceUrl } = useViewState();
  const resolvedTab = activeTab && (TAB_KEYS as readonly string[]).includes(activeTab)
    ? activeTab
    // 委派任务无环路环节视图，执行发生在讨论区，默认落到「讨论」Tab。
    : (detail?.task.execution_mode === 'delegate' ? 'discussion' : 'overview');

  // 拉取任务详情（含基本 loop 信息）。
  useEffect(() => {
    let alive = true;
    setLoading(true);
    bundledApi.getTaskDetail(workspaceId, taskId)
      .then((raw) => {
        if (!alive) return;
        const d = raw as TaskDetailData;
        setDetail(d);
        if (onTitleReady && d.task?.title) onTitleReady(d.task.title);
      })
      .catch(() => { if (alive) message.error('加载任务详情失败'); })
      .finally(() => { if (alive) setLoading(false); });
    return () => { alive = false; };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [taskId, workspaceId]);

  // 任务详情加载后，若有 loop_id 则并行拉取完整 LoopDetail。
  useEffect(() => {
    if (!detail) return;
    const lpId = detail.task.loop_id ?? detail.loop?.id;
    if (!lpId) return;
    let alive = true;
    setLoopLoading(true);
    const wsId = detail.task.workspace_id ?? detail.loop?.workspace_id ?? workspaceId;
    dbLoops.getLoop(wsId, lpId)
      .then((ld) => { if (alive) setLoopDetail(ld); })
      .catch(() => { /* 环路加载失败不影响任务展示 */ })
      .finally(() => { if (alive) setLoopLoading(false); });
    return () => { alive = false; };
  }, [detail, workspaceId]);

  // 删除环路。
  const handleDelete = useCallback(async () => {
    if (!loopDetail) return;
    const wsId = loopDetail.workspace_id ?? workspaceId;
    try {
      await dbLoops.deleteLoop(wsId, loopDetail.id);
      message.success('已删除');
      onLoopChanged?.();
    } catch {
      message.error('删除失败，环路可能正在被引用');
    }
  }, [loopDetail, workspaceId, onLoopChanged]);

  // 打开再次执行 Modal。
  const openReqModal = () => {
    if (detail) setNewRequirement(detail.task.description ?? detail.task.title);
    setReqModalOpen(true);
  };

  // 提交新执行。
  const handleNewExec = async () => {
    if (!newRequirement.trim()) { message.warning('请输入需求'); return; }
    setTriggering(true);
    try {
      await bundledApi.createTaskExecution(workspaceId, taskId, newRequirement);
      message.success('新执行已创建');
      setReqModalOpen(false);
      setNewRequirement('');
      const raw = await bundledApi.getTaskDetail(workspaceId, taskId) as TaskDetailData;
      setDetail(raw);
      onTriggered?.();
    } catch { message.error('创建失败'); }
    finally { setTriggering(false); }
  };

  // 加载态。
  if (loading) return <Spin style={{ display: 'block', margin: '40px auto' }} />;
  if (!detail) return <Empty description="暂无任务详情" style={{ marginTop: 48 }} />;

  const { task, template } = detail;
  const lpId = task.loop_id ?? detail.loop?.id ?? 0;
  const lpWsId = task.workspace_id ?? detail.loop?.workspace_id ?? null;

  const tabItems = [
    {
      key: 'overview',
      label: '概览',
      children: (
        <OverviewTab
          task={task} template={template}
          loopDetail={loopDetail} projectDirs={projectDirs}
        />
      ),
    },
    {
      key: 'dag',
      label: `执行环路 (${loopDetail?.steps?.length ?? 0})`,
      children: <DAGTab loopDetail={loopDetail} steps={detail.steps ?? []} onOpenTodo={onOpenTodo} />,
    },
    {
      key: 'exec',
      label: '执行历史',
      children: (
        <ExecHistoryTab loopId={lpId} workspaceId={lpWsId} loopName={loopDetail?.name ?? task.title} />
      ),
    },
    {
      // 任务讨论区（需求 060）：论坛跟帖 + @专家/@执行器 触发执行后回帖。
      // forceRender：保证「讨论」Tab 非 active 时 DiscussionTab 仍挂载、持续上报 running 数，角标才可见。
      key: 'discussion',
      label: <Badge count={discussionRunning} offset={[10, 0]} size="small">讨论</Badge>,
      forceRender: true,
      children: (
        <DiscussionTab
          taskId={task.id}
          workspaceId={task.workspace_id ?? lpWsId ?? workspaceId}
          onRunningCountChange={setDiscussionRunning}
        />
      ),
    },
  ];

  return (
    <div className={styles.panel}>
      <DetailHeader
        task={task} template={template} loopDetail={loopDetail}
        onExecute={openReqModal} onDelete={handleDelete}
      />
      <div className={styles.tabsWrap}>
        <Tabs
          items={tabItems}
          activeKey={resolvedTab}
          onChange={(key) => replaceUrl('tasks', { id: task.id, tab: key })}
          style={{ height: '100%' }}
          tabBarExtraContent={loopLoading ? <Spin size="small" style={{ marginRight: 16 }} /> : undefined}
        />
      </div>

      <Modal
        title="输入这次的需求"
        open={reqModalOpen}
        onCancel={() => setReqModalOpen(false)}
        onOk={handleNewExec}
        confirmLoading={triggering}
        okText="开始执行"
      >
        <Input.TextArea value={newRequirement} onChange={(e) => setNewRequirement(e.target.value)} rows={4} />
      </Modal>
    </div>
  );
}
