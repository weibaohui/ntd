import { useEffect, useState } from 'react';
import { Button, Dropdown, Modal, Select, Tag, message } from 'antd';
import {
  PlayCircleOutlined,
  MoreOutlined,
  InboxOutlined,
  ThunderboltOutlined,
  ClockCircleOutlined,
  RetweetOutlined,
  LinkOutlined,
} from '@ant-design/icons';
import type { MenuProps } from 'antd';
import * as db from '@/utils/database';
import { formatRelativeTime } from '@/utils/datetime';
import { SchedulerSection } from '@/components/todo-drawer/SchedulerSection';
import { DEFAULT_CRON } from '@/components/todo-drawer/constants';
import type { TodoCenterItem, ComputedBucket } from '@/types';

/** 各驱动分类的展示标签：中文名 + antd Tag 颜色。集中管理避免散落。 */
const BUCKET_DISPLAY: Record<ComputedBucket, { label: string; color: string }> = {
  manual: { label: '手动触发', color: 'blue' },
  time_driven: { label: '时间驱动', color: 'cyan' },
  event_driven: { label: '事件驱动', color: 'purple' },
  loop_driven: { label: 'Loop 驱动', color: 'geekblue' },
  archived: { label: '已归档', color: 'default' },
};

/** 来源提示：把 action_type 翻译成可读来源名，未匹配则不展示。 */
export function sourceLabel(actionType?: string | null): string | null {
  if (!actionType) return null;
  const map: Record<string, string> = {
    blackboard: '黑板',
    blackboard_propose: '黑板推荐',
    title_optimize: '标题优化',
    prompt_optimize: 'Prompt 优化',
  };
  return map[actionType] ?? actionType;
}

interface TodoCenterCardProps {
  item: TodoCenterItem;
  /** 任意变更（执行/归档/恢复/webhook/调度）后回调，让页面重拉列表保持口径一致。 */
  onChanged: () => void;
  /** 点击卡片主体进入现有事项详情页。 */
  onSelectTodo: (id: number) => void;
  /** Phase 5：点击所属 Loop 跳转 Loop 详情。 */
  onSelectLoop: (loopId: number) => void;
}

/**
 * 事项中心卡片：单张卡片承载一个事项的浏览与低成本操作。
 * 设计原则（设计文档风险四）：卡片只放一个主操作，次要操作进「更多」菜单，
 * 避免重蹈当前密集列表「操作按钮挤在行里」的覆辙。
 */
export function TodoCenterCard({ item, onChanged, onSelectTodo, onSelectLoop }: TodoCenterCardProps) {
  // busy 覆盖所有异步操作（执行/归档/恢复/webhook/调度/复制/移动），防止并发重复点击
  const [busy, setBusy] = useState(false);
  const [schedOpen, setSchedOpen] = useState(false);
  // 调度弹窗本地态：打开时由当前 scheduler 字段初始化，确认后才落库（避免半成品持久化）
  const [schedEnabled, setSchedEnabled] = useState(true);
  const [schedConfig, setSchedConfig] = useState<string>(DEFAULT_CRON);
  // 复制/移动工作空间弹窗：mode=null 表示关闭，两种模式共用一个弹窗
  const [wsMode, setWsMode] = useState<'copy' | 'move' | null>(null);
  // 已归档卡片主操作是「恢复」；其余卡片主操作随 status 切换（见 mainAction）
  const isArchived = item.computed_bucket === 'archived';

  // 共用 loading + 错误处理的轻量变更包装器。
  const runMutation = async (label: string, fn: () => Promise<unknown>) => {
    setBusy(true);
    try {
      await fn();
      message.success(`${label}成功`);
      onChanged();
    } catch (e) {
      message.error(`${label}失败：${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setBusy(false);
    }
  };

  // 打开调度弹窗：用当前已有配置初始化，没有则用默认 cron。
  const openSchedulerModal = () => {
    setSchedEnabled(item.scheduler_enabled ?? true);
    setSchedConfig(item.scheduler_config || DEFAULT_CRON);
    setSchedOpen(true);
  };

  // 弹窗确认：落库调度配置（设为/恢复时间驱动走同一条 PUT /scheduler）。
  const saveScheduler = () =>
    runMutation('保存调度', () => db.updateScheduler(item.id, schedEnabled, schedConfig || null));

  const menuItems = buildMenuItems(
    item,
    isArchived,
    runMutation,
    openSchedulerModal,
    (mode) => setWsMode(mode),
  );

  // 主操作随状态切换（设计文档卡片主操作规则）：
  // 已归档→恢复；运行中→查看运行（跳详情）；失败→查看失败（跳详情）；待执行/已完成→执行一次
  const mainAction = isArchived ? (
    <Button
      size="small"
      icon={<InboxOutlined />}
      loading={busy}
      onClick={(e) => {
        e.stopPropagation();
        runMutation('恢复', () => db.restoreTodo(item.id));
      }}
    >
      恢复
    </Button>
  ) : item.status === 'running' || item.status === 'failed' ? (
    <Button
      size="small"
      icon={<PlayCircleOutlined />}
      onClick={(e) => {
        e.stopPropagation();
        onSelectTodo(item.id);
      }}
    >
      {item.status === 'running' ? '查看运行' : '查看失败'}
    </Button>
  ) : (
    <Button
      type="primary"
      size="small"
      icon={<PlayCircleOutlined />}
      loading={busy}
      onClick={(e) => {
        e.stopPropagation();
        runMutation('执行', () => db.executeTodo(item.id));
      }}
    >
      执行一次
    </Button>
  );

  // 时间驱动卡片的调度活跃状态：用于控制左侧强调条、标签颜色等视觉区分
  // scheduler_enabled 未定义时默认视为开启，与调度弹窗初始化逻辑（line 83）保持一致
  const isTimeDrivenActive = item.computed_bucket === 'time_driven' && item.scheduler_enabled !== false;

  return (
    <div
      className={`todo-center-card ${isTimeDrivenActive ? 'todo-center-card--time-active' : ''}`}
      // React Portal 会把 Dropdown 菜单的合成事件冒泡回卡片（即便菜单 DOM 在 body），
      // 因此不能只靠按钮 stopPropagation——点菜单项仍会触发卡片跳详情。
      // 这里用 target 检测：点击源自按钮/下拉/弹窗时视为操作意图，不跳详情。
      onClick={(e) => {
        if ((e.target as HTMLElement).closest('button, .ant-dropdown, .ant-modal, .ant-select')) return;
        onSelectTodo(item.id);
      }}
      role="button"
      data-testid={`todo-center-card-${item.id}`}
    >
      <div className="todo-center-card-head">
        <span className="todo-center-card-title">
          <span className="todo-center-card-id">#{item.id}</span>
          {item.title}
        </span>
        <Dropdown menu={{ items: menuItems }} trigger={['click']} placement="bottomRight">
          <Button
            type="text"
            size="small"
            icon={<MoreOutlined />}
            onClick={(e) => e.stopPropagation()}
            aria-label="更多操作"
          />
        </Dropdown>
      </div>

      <div className="todo-center-card-tags">
        {/* 时间驱动且已暂停时，标签置灰以弱化视觉权重；活跃时用原青色强调 */}
        {/* scheduler_enabled 未定义时默认视为开启，只有显式 false 才视为已暂停 */}
        {item.computed_bucket === 'time_driven' && item.scheduler_enabled === false ? (
          <>
            <Tag color="default">{BUCKET_DISPLAY[item.computed_bucket].label}</Tag>
            <Tag color="default">已暂停</Tag>
          </>
        ) : (
          <Tag color={BUCKET_DISPLAY[item.computed_bucket].color}>
            {BUCKET_DISPLAY[item.computed_bucket].label}
          </Tag>
        )}
        <StatusTag status={item.status} />
        {sourceLabel(item.action_type) && <Tag color="gold">{sourceLabel(item.action_type)}</Tag>}
        {/* 绑定斜杠命令：手动触发仍属手动分类（命令是执行入口，非持续驱动） */}
        {item.bound_slash_command && (
          <Tag color="geekblue">绑定命令 {item.bound_slash_command}</Tag>
        )}
        {item.webhook_enabled && item.computed_bucket !== 'event_driven' && (
          <Tag color="purple" icon={<ThunderboltOutlined />}>兼事件</Tag>
        )}
        {item.scheduler_config && item.computed_bucket !== 'time_driven' && (
          <Tag color="cyan" icon={<ClockCircleOutlined />}>兼时间</Tag>
        )}
      </div>

      <CardMeta item={item} onSelectLoop={onSelectLoop} />

      <div className="todo-center-card-foot">
        <span className="todo-center-card-time">
          {item.archived_at
            ? `归档于 ${formatRelativeTime(item.archived_at)}`
            : `更新于 ${formatRelativeTime(item.updated_at)}`}
        </span>
        {mainAction}
      </div>

      <SchedulerModal
        open={schedOpen}
        enabled={schedEnabled}
        config={schedConfig}
        existingConfig={item.scheduler_config}
        onEnabledChange={setSchedEnabled}
        onConfigChange={setSchedConfig}
        onCancel={() => setSchedOpen(false)}
        onOk={() => {
          setSchedOpen(false);
          saveScheduler();
        }}
      />

      <WorkspaceMoveCopyModal
        todoId={item.id}
        mode={wsMode}
        currentWorkspaceId={item.workspace_id ?? null}
        onClose={() => setWsMode(null)}
        onDone={onChanged}
      />
    </div>
  );
}

/** 状态 Tag：把后端 status 串映射成中文 + 颜色，复用事项列表口径。 */
function StatusTag({ status }: { status?: string }) {
  const map: Record<string, { label: string; color: string }> = {
    pending: { label: '待执行', color: 'default' },
    running: { label: '运行中', color: 'processing' },
    completed: { label: '已完成', color: 'success' },
    failed: { label: '失败', color: 'error' },
  };
  const entry = status ? map[status] : undefined;
  if (!entry) return null;
  return <Tag color={entry.color}>{entry.label}</Tag>;
}

/** 卡片中部元信息：按分类展示该分类用户最关心的字段。 */
function CardMeta({
  item,
  onSelectLoop,
}: {
  item: TodoCenterItem;
  onSelectLoop: (loopId: number) => void;
}) {
  const failCount = item.consecutive_failure_count ?? 0;
  // 时间驱动且调度已启用：视为活跃状态，视觉上高亮下次运行时间
  // scheduler_enabled 未定义时默认视为开启，与调度弹窗初始化逻辑保持一致
  const isTimeActive = item.computed_bucket === 'time_driven' && item.scheduler_enabled !== false;
  // 时间驱动但调度已暂停：弱化调度表达式显示，降低视觉权重
  // 只有显式 false 才视为已暂停
  const isTimePaused = item.computed_bucket === 'time_driven' && item.scheduler_enabled === false;

  return (
    <div className="todo-center-card-meta">
      {item.computed_bucket === 'time_driven' && item.scheduler_config && (
        <MetaLine
          icon={<ClockCircleOutlined />}
          text={`调度 ${item.scheduler_config}`}
          muted={isTimePaused}
        />
      )}
      {isTimeActive && item.scheduler_next_run_at && (
        <MetaLine
          text={`下次运行 ${formatRelativeTime(item.scheduler_next_run_at)}`}
          highlight="primary"
        />
      )}
      {/* 事件驱动卡片：Webhook 入口路径 + 最近触发时间（webhook 专属，不受手动执行影响） */}
      {item.computed_bucket === 'event_driven' && (
        <MetaLine icon={<LinkOutlined />} text={`/webhook/trigger/todo/${item.id}`} />
      )}
      {item.computed_bucket === 'event_driven' && item.last_webhook_trigger_at && (
        <MetaLine text={`最近触发 ${formatRelativeTime(item.last_webhook_trigger_at)}`} />
      )}
      {/* Loop 驱动卡片展示所属 Loop，点击跳转 Loop 详情 */}
      {item.computed_bucket === 'loop_driven' && (
        <ReferencingLoops item={item} onSelectLoop={onSelectLoop} />
      )}
      {item.last_execution_status && (
        <MetaLine text={`最近执行 ${item.last_execution_status}${item.last_execution_at ? ` · ${formatRelativeTime(item.last_execution_at)}` : ''}`} />
      )}
      {/* 连续失败次数 > 0 时醒目提示（时间/事件驱动健康度） */}
      {failCount > 0 && (
        <MetaLine text={`连续失败 ${failCount} 次`} />
      )}
    </div>
  );
}

/** 所属 Loop：把 referencing_loops 渲染为可点击的小标签，点击跳 Loop 详情。 */
function ReferencingLoops({
  item,
  onSelectLoop,
}: {
  item: TodoCenterItem;
  onSelectLoop: (loopId: number) => void;
}) {
  const loops = item.referencing_loops ?? [];
  if (loops.length === 0) {
    return <MetaLine icon={<RetweetOutlined />} text={`被 ${item.used_by_loop_step_count} 个启用环节引用`} />;
  }
  return (
    <div className="todo-center-card-meta-line">
      <RetweetOutlined />
      {loops.map((l) => (
        <Tag
          key={l.loop_id}
          color="geekblue"
          style={{ cursor: 'pointer' }}
          onClick={(e) => {
            e.stopPropagation();
            onSelectLoop(l.loop_id);
          }}
        >
          {l.loop_name}
        </Tag>
      ))}
    </div>
  );
}

/**
 * 单行元信息：可选图标 + 文本。
 * - muted：弱化显示（暂停/非活跃状态用）
 * - highlight：高亮显示（活跃/重要信息用，primary = 主色强调）
 */
function MetaLine({
  icon,
  text,
  muted,
  highlight,
}: {
  icon?: React.ReactNode;
  text: string;
  muted?: boolean;
  highlight?: 'primary';
}) {
  const className = [
    'todo-center-card-meta-line',
    muted ? 'todo-center-card-meta-line--muted' : '',
    highlight === 'primary' ? 'todo-center-card-meta-line--highlight-primary' : '',
  ].filter(Boolean).join(' ');
  return (
    <div className={className}>
      {icon && <span className="todo-center-card-meta-icon">{icon}</span>}
      <span>{text}</span>
    </div>
  );
}

/** 调度配置弹窗：复用 SchedulerSection（react-js-cron 编辑器），设为/编辑时间驱动共用。 */
function SchedulerModal({
  open,
  enabled,
  config,
  existingConfig,
  onEnabledChange,
  onConfigChange,
  onCancel,
  onOk,
}: {
  open: boolean;
  enabled: boolean;
  config: string;
  existingConfig?: string | null;
  onEnabledChange: (v: boolean) => void;
  onConfigChange: (v: string) => void;
  onCancel: () => void;
  onOk: () => void;
}) {
  return (
    <Modal
      title="时间驱动调度配置"
      open={open}
      onOk={onOk}
      onCancel={onCancel}
      okText="保存"
      cancelText="取消"
      destroyOnClose
    >
      <SchedulerSection
        enabled={enabled}
        config={config}
        onEnabledChange={onEnabledChange}
        onConfigChange={onConfigChange}
        existingConfig={existingConfig}
      />
    </Modal>
  );
}

/** 构建「更多」菜单项。按是否归档/是否时间驱动分支，保持单函数简短。 */
function buildMenuItems(
  item: TodoCenterItem,
  isArchived: boolean,
  runMutation: (label: string, fn: () => Promise<unknown>) => void,
  openSchedulerModal: () => void,
  openWorkspacePicker: (mode: 'copy' | 'move') => void,
): MenuProps['items'] {
  if (isArchived) {
    // 已归档：恢复 + 删除（删除走确认；被 Loop 引用时后端会拒绝）
    return [
      {
        key: 'restore',
        label: '恢复事项',
        onClick: () => runMutation('恢复', () => db.restoreTodo(item.id)),
      },
      deleteMenuItem(item, runMutation),
    ];
  }
  // Loop 驱动卡片不提供复制/移动（设计文档 Loop 驱动操作列表不含这两项）：
  // Loop 归属只能由 Loop 配置改变，复制/移动绕过流程结构不合理。
  const workspaceItems: NonNullable<MenuProps['items']> =
    item.computed_bucket === 'loop_driven'
      ? []
      : [
          { type: 'divider' as const, key: 'div_ws' },
          { key: 'copy', label: '复制到工作空间', onClick: () => openWorkspacePicker('copy') },
          { key: 'move', label: '移动到工作空间', onClick: () => openWorkspacePicker('move') },
        ];
  return [
    archiveMenuItem(item, runMutation),
    ...timeDrivenMenuItems(item, runMutation, openSchedulerModal),
    webhookMenuItem(item, runMutation),
    ...workspaceItems,
  ];
}

/** 删除菜单项：Modal.confirm 二次确认。被 Loop 引用时后端返回 400，runMutation 会提示。 */
function deleteMenuItem(
  item: TodoCenterItem,
  runMutation: (label: string, fn: () => Promise<unknown>) => void,
): NonNullable<MenuProps['items']>[number] {
  return {
    key: 'delete',
    label: '删除',
    danger: true,
    onClick: () =>
      Modal.confirm({
        title: '确认删除该事项？',
        content: '删除为软删除，不可在事项中心恢复。被 Loop 引用时后端会拒绝删除。',
        okText: '删除',
        okButtonProps: { danger: true },
        cancelText: '取消',
        onOk: () => runMutation('删除', () => db.deleteTodo(item.id)),
      }),
  };
}

/** 归档菜单项：被 Loop 引用时给出更强的归档不解除引用提示（设计文档风险三）。 */
function archiveMenuItem(
  item: TodoCenterItem,
  runMutation: (label: string, fn: () => Promise<unknown>) => void,
): NonNullable<MenuProps['items']>[number] {
  const loopHint =
    item.used_by_loop_step_count > 0
      ? `该事项仍被 ${item.used_by_loop_step_count} 个启用的 Loop 环节引用，归档不会解除引用。`
      : '归档不删除数据，也不解除 Loop 引用。';
  return {
    key: 'archive',
    label: '归档',
    onClick: () =>
      Modal.confirm({
        title: '确认归档该事项？',
        content: `${loopHint}已归档事项可在「已归档」分类恢复。`,
        okText: '归档',
        cancelText: '取消',
        onOk: () => runMutation('归档', () => db.archiveTodo(item.id)),
      }),
  };
}

/** 时间驱动菜单项：设为/暂停/恢复/取消。scheduler_config 为空=尚未时间驱动。 */
function timeDrivenMenuItems(
  item: TodoCenterItem,
  runMutation: (label: string, fn: () => Promise<unknown>) => void,
  openSchedulerModal: () => void,
): NonNullable<MenuProps['items']> {
  if (!item.scheduler_config) {
    // 未配置调度：仅提供「设为时间驱动」
    return [
      { key: 'set_time', label: '设为时间驱动', onClick: openSchedulerModal },
    ];
  }
  // 已有调度：暂停/恢复（切换 enabled）+ 取消（清空 config）
  // scheduler_enabled 未定义时默认视为开启，只有显式 false 才视为已暂停状态
  const pauseResume = item.scheduler_enabled !== false
    ? {
        key: 'pause_time',
        label: '暂停时间驱动',
        // 暂停：关 enabled，保留 config（仍属时间驱动，卡片标已暂停）
        onClick: () => runMutation('暂停时间驱动', () => db.updateScheduler(item.id, false, item.scheduler_config ?? null)),
      }
    : {
        key: 'resume_time',
        label: '恢复时间驱动',
        // 恢复：开 enabled，保留 config
        onClick: () => runMutation('恢复时间驱动', () => db.updateScheduler(item.id, true, item.scheduler_config ?? null)),
      };
  const edit = { key: 'edit_time', label: '编辑调度配置', onClick: openSchedulerModal };
  const cancel = {
    key: 'cancel_time',
    label: '取消时间驱动',
    onClick: () =>
      Modal.confirm({
        title: '确认取消时间驱动？',
        content: '将清空调度配置。若未被 Loop 引用、未启用 Webhook，事项回到手动触发。',
        okText: '取消时间驱动',
        cancelText: '保留',
        // 取消：关 enabled 且清空 config
        onOk: () => runMutation('取消时间驱动', () => db.updateScheduler(item.id, false, null)),
      }),
  };
  return [pauseResume, edit, cancel];
}

/** 事件驱动菜单项：开启/关闭 webhook，与 scheduler 端点对称。 */
function webhookMenuItem(
  item: TodoCenterItem,
  runMutation: (label: string, fn: () => Promise<unknown>) => void,
): NonNullable<MenuProps['items']>[number] {
  const on = item.webhook_enabled;
  return {
    key: 'webhook',
    label: on ? '关闭事件驱动' : '设为事件驱动',
    onClick: () => runMutation(on ? '关闭事件驱动' : '设为事件驱动', () => db.updateTodoWebhook(item.id, !on)),
  };
}

/**
 * 复制/移动到工作空间弹窗。
 * 复用现有的 batchCopyTodosWorkspace / batchMoveTodosWorkspace 接口（单条用 [todoId]）。
 * 排除当前工作空间：移动到原处无意义；复制到原处会产生重复。
 */
function WorkspaceMoveCopyModal({
  todoId,
  mode,
  currentWorkspaceId,
  onClose,
  onDone,
}: {
  todoId: number;
  mode: 'copy' | 'move' | null;
  currentWorkspaceId: number | null;
  onClose: () => void;
  onDone: () => void;
}) {
  const [workspaces, setWorkspaces] = useState<{ id: number; name: string; path: string }[]>([]);
  const [targetId, setTargetId] = useState<number | null>(null);
  const [loading, setLoading] = useState(false);

  // 打开时拉取工作空间列表，并默认选第一个非当前工作空间
  useEffect(() => {
    if (mode === null) return;
    db.getProjectDirectories()
      .then((dirs) => {
        setWorkspaces(dirs.map((d) => ({ id: d.id, name: d.name || d.path, path: d.path })));
        const first = dirs.find((d) => d.id !== currentWorkspaceId);
        setTargetId(first ? first.id : null);
      })
      .catch(() => setWorkspaces([]));
  }, [mode, currentWorkspaceId]);

  const handleOk = async () => {
    if (targetId == null) return;
    setLoading(true);
    try {
      if (mode === 'copy') {
        await db.batchCopyTodosWorkspace([todoId], targetId);
      } else if (mode === 'move') {
        await db.batchMoveTodosWorkspace([todoId], targetId);
      }
      message.success(mode === 'copy' ? '已复制' : '已移动');
      onDone();
      onClose();
    } catch (e) {
      message.error(`操作失败：${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setLoading(false);
    }
  };

  return (
    <Modal
      title={mode === 'copy' ? '复制到工作空间' : '移动到工作空间'}
      open={mode !== null}
      onOk={handleOk}
      onCancel={onClose}
      okText={mode === 'copy' ? '复制' : '移动'}
      cancelText="取消"
      confirmLoading={loading}
      destroyOnClose
    >
      <Select
        style={{ width: '100%' }}
        placeholder="选择目标工作空间"
        value={targetId ?? undefined}
        onChange={setTargetId}
        options={workspaces.map((w) => ({
          value: w.id,
          label: w.name,
          disabled: w.id === currentWorkspaceId,
        }))}
      />
    </Modal>
  );
}
