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
  message, Modal, Input, InputNumber, Popover, Empty, Popconfirm,
} from 'antd';
import {
  ThunderboltOutlined, DeleteOutlined, EditOutlined,
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
  task: { id: number; title: string; status: string; description?: string; workspace_id?: number; loop_id?: number; execution_mode?: string; assignee_kind?: string; assignee_name?: string; auto_continue?: boolean; continue_rounds?: number; delegate_max_rounds?: number | null; delegate_max_rounds_effective?: number; delegate_max_rounds_fallback?: number };
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
 * 自动接力进度徽标（需求 092）。仅管家接力任务显示，文案「管家调度中 N/M」：
 * - M（上限）来自后端三级解析的有效值 delegate_max_rounds_effective，前端不再硬编码 10；
 * - 未达上限用 processing 蓝，达上限用 warning 橙（与后端 HitLimit 说明帖呼应，提示接管）；
 * - 点击 ✎ 弹出 [RelayMaxEditor] 内联调整该任务的上限覆盖（运行时可改）。
 * 非接力任务返回 null。本组件不放 hooks：纯条件渲染，避免「early return 在 hook 之前」违规。
 */
function RelayBadge({
  task, onUpdateMax,
}: {
  task: TaskDetailData['task'];
  onUpdateMax: (max: number | null) => Promise<void>;
}) {
  if (!isAutoRelayTask(task)) return null;
  return <RelayMaxEditor task={task} onUpdateMax={onUpdateMax} />;
}

/**
 * 徽标内联编辑器：Tag 本体 + Popover（InputNumber 调上限 + 「恢复默认」）。
 * 打开时以当前 raw 覆盖(delegate_max_rounds)回显，null 显示空（=用默认）；确定/恢复均经
 * onUpdateMax 落库并重拉详情，effective 随之刷新，徽标 M 实时同步。
 */
function RelayMaxEditor({
  task, onUpdateMax,
}: {
  task: TaskDetailData['task'];
  onUpdateMax: (max: number | null) => Promise<void>;
}) {
  const rounds = task.continue_rounds ?? 0;
  // effective 兜底 10 仅为类型安全：后端恒返回该字段，缺失属异常态。
  const effectiveMax = task.delegate_max_rounds_effective ?? 10;
  // 「清除任务覆盖后」的回退值（工作空间默认或兜底常量）：placeholder / 留空提示读它。
  // 不能用 effective：任务已有覆盖时 effective 即覆盖值本身，提示「留空=用工作空间默认（N 轮）」会误显成
  // 即将被清除的那个覆盖值，与「清除后实际回退到工作空间默认」矛盾（Spec 评审发现）。
  const fallbackMax = task.delegate_max_rounds_fallback ?? effectiveMax;
  // >=：与后端 plan_delegate_relay 的 `rounds >= max` 护栏口径一致（设计 §5.2）。
  const atLimit = rounds >= effectiveMax;
  // Popover 受控开关 + 输入态：每次打开以最新 raw 回显，避免上一次残留值误导。
  const [open, setOpen] = useState(false);
  const [editing, setEditing] = useState<number | null>(task.delegate_max_rounds ?? null);
  const [saving, setSaving] = useState(false);

  // 落库并刷新：仅当 onUpdateMax 成功 resolve 才关 Popover；失败时 onUpdateMax 会向上抛错，
  // 跳过 setOpen(false)，编辑器保持打开供改值重试（错误提示已由 handleUpdateMax 的 catch 弹出）。
  // 此处 catch 兜住错误只复位 saving，避免 onClick 内产生未处理 promise rejection。
  const submit = async (value: number | null) => {
    setSaving(true);
    try {
      await onUpdateMax(value);
      setOpen(false);
    } catch {
      // 不关 Popover：让用户在原值基础上修正后重试（message.error 已在 handleUpdateMax 弹过）。
    } finally {
      setSaving(false);
    }
  };
  // 是否已有覆盖（决定「恢复默认」可点）；确定仅在「填了且与当前覆盖不同」时可点。
  const hasOverride = task.delegate_max_rounds != null;
  const confirmDisabled = editing == null || editing === task.delegate_max_rounds;

  return (
    <Popover
      trigger="click"
      placement="bottom"
      open={open}
      onOpenChange={(o) => {
        setOpen(o);
        // 每次打开重置输入为当前 raw 覆盖，关闭再开不会带上次未提交的脏值。
        if (o) setEditing(task.delegate_max_rounds ?? null);
      }}
      title="调整接力轮数上限"
      content={
        <Space direction="vertical" style={{ width: 240 }}>
          <InputNumber
            min={1}
            max={50}
            value={editing}
            onChange={(v) => setEditing(v ?? null)}
            placeholder={`默认 ${fallbackMax} 轮`}
            style={{ width: '100%' }}
            data-testid="relay-max-input"
          />
          <Space>
            <Button
              size="small"
              type="primary"
              loading={saving}
              disabled={confirmDisabled}
              onClick={() => editing != null && submit(editing)}
              data-testid="relay-max-confirm"
            >
              确定
            </Button>
            <Button
              size="small"
              disabled={!hasOverride || saving}
              onClick={() => submit(null)}
              data-testid="relay-max-reset"
            >
              恢复默认
            </Button>
          </Space>
          <Text type="secondary" style={{ fontSize: 12 }}>
            留空=用工作空间默认（{fallbackMax} 轮）；达上限后停止自动接力。
          </Text>
        </Space>
      }
    >
      <Tag
        color={atLimit ? 'warning' : 'processing'}
        style={{ cursor: 'pointer' }}
        data-testid="relay-badge"
      >
        管家调度中 {rounds}/{effectiveMax} <EditOutlined style={{ marginLeft: 4 }} />
      </Tag>
    </Popover>
  );
}

/** 顶部条：标题 + 状态/复杂度 + 元信息 + 删除 + 再次执行。 */
function DetailHeader({
  task, template, loopDetail, onExecute, onDelete, onUpdateMax,
}: {
  task: TaskDetailData['task'];
  template?: TaskDetailData['template'];
  loopDetail: LoopDetail | null;
  onExecute: () => void;
  onDelete: () => void;
  // 接力上限内联编辑落库（透传给 RelayBadge）；仅管家接力任务的徽标会用。
  onUpdateMax: (max: number | null) => Promise<void>;
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
          <RelayBadge task={task} onUpdateMax={onUpdateMax} />
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

  // 内联调整某任务的接力上限覆盖（需求 092）：落库后重拉详情，effective 随之刷新，徽标 M 同步。
  // 定义在 early return 之前并用 useCallback，避免「条件 hook」违规；用 taskId 而非 detail.task，
  // 保证 detail 未就绪时也不会引用空对象。
  const handleUpdateMax = useCallback(async (max: number | null) => {
    try {
      await bundledApi.updateTask(workspaceId, taskId, { delegate_max_rounds: max });
      const raw = await bundledApi.getTaskDetail(workspaceId, taskId) as TaskDetailData;
      setDetail(raw);
      message.success(max == null ? '已恢复默认上限' : `上限已设为 ${max} 轮`);
      onTriggered?.();
    } catch (e) {
      // updateTask 400 时后端返回中文 message（越界/非委派），拦截器已透传，先弹给用户可见提示。
      message.error(e instanceof Error ? e.message : '更新接力上限失败');
      // 再向上抛错：RelayMaxEditor.submit 据此判定失败、跳过关 Popover 的语句，让用户在原
      // Popover 内改值重试。否则 PATCH 失败却关弹层，用户须重新点开 ✎，体验割裂（评审发现）。
      throw e;
    }
  }, [workspaceId, taskId, onTriggered]);

  // 加载态。
  if (loading) return <Spin style={{ display: 'block', margin: '40px auto' }} />;
  if (!detail) return <Empty description="暂无任务详情" style={{ marginTop: 48 }} />;

  const { task, template } = detail;
  const lpId = task.loop_id ?? detail.loop?.id ?? 0;
  const lpWsId = task.workspace_id ?? detail.loop?.workspace_id ?? null;

  // 委派任务（创建时选「委派」而非「工艺环路」）未绑定环路，没有「执行环路」「执行历史」可展示：
  // 此前这两个 Tab 会渲染成空状态（「暂无关联环路」「暂无执行环路」），对用户无意义且易误导。
  // 这里按 execution_mode 条件组装——委派任务只保留「概览」「讨论」；判断口径与 Header 处理人、
  // 隐藏「再次执行」、默认 Tab 落「讨论」等既有委派分支完全一致，不引入新概念。
  const isDelegate = task.execution_mode === 'delegate';

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
    // 「执行环路」「执行历史」强依赖 task.loop_id：仅工艺环路任务才纳入这两个 Tab。
    // 委派任务展开为空数组，等价于这两项不出现（顺序仍夹在「概览」与「讨论」之间）。
    ...(!isDelegate ? [
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
    ] : []),
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

  // 实际渲染的 Tab key 集合（委派任务不含 dag/exec）。resolvedTab 依据 TAB_KEYS 白名单解析 URL ?tab=，
  // 而该白名单恒含 dag/exec；委派任务若 URL 残留 ?tab=dag，解析出的 key 会指向已隐藏的 Tab，Ant Design
  // Tabs 随即落到无选中态、内容区空白。此处校验命中即回退默认 Tab，避免空白页。
  const visibleTabKeys: string[] = tabItems.map((t) => t.key);
  const activeTabKey = visibleTabKeys.includes(resolvedTab)
    ? resolvedTab
    : (isDelegate ? 'discussion' : 'overview');

  return (
    <div className={styles.panel}>
      <DetailHeader
        task={task} template={template} loopDetail={loopDetail}
        onExecute={openReqModal} onDelete={handleDelete} onUpdateMax={handleUpdateMax}
      />
      <div className={styles.tabsWrap}>
        <Tabs
          items={tabItems}
          activeKey={activeTabKey}
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
