import { useState } from 'react';
import { Button, Dropdown, Modal, Tag, message } from 'antd';
import {
  PlayCircleOutlined,
  MoreOutlined,
  InboxOutlined,
  ThunderboltOutlined,
  ClockCircleOutlined,
  RetweetOutlined,
} from '@ant-design/icons';
import type { MenuProps } from 'antd';
import * as db from '@/utils/database';
import { formatRelativeTime } from '@/utils/datetime';
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
function sourceLabel(actionType?: string | null): string | null {
  if (!actionType) return null;
  const map: Record<string, string> = {
    blackboard: '黑板',
    title_optimize: '标题优化',
    prompt_optimize: 'Prompt 优化',
  };
  return map[actionType] ?? actionType;
}

interface TodoCenterCardProps {
  item: TodoCenterItem;
  /** 任意变更（执行/归档/恢复/webhook）后回调，让页面重拉列表保持口径一致。 */
  onChanged: () => void;
  /** 点击卡片主体进入现有事项详情页。 */
  onSelectTodo: (id: number) => void;
}

/**
 * 事项中心卡片：单张卡片承载一个事项的浏览与低成本操作。
 * 设计原则（设计文档风险四）：卡片只放一个主操作，次要操作进「更多」菜单，
 * 避免重蹈当前密集列表「操作按钮挤在行里」的覆辙。
 */
export function TodoCenterCard({ item, onChanged, onSelectTodo }: TodoCenterCardProps) {
  const [busy, setBusy] = useState(false);
  const isArchived = item.computed_bucket === 'archived';

  // 执行一次：手动触发一次性执行。归档卡片不提供此操作（归档主操作是「恢复」）。
  const handleExecute = async () => {
    setBusy(true);
    try {
      await db.executeTodo(item.id);
      message.success('任务已开始执行');
      onChanged();
    } catch (e) {
      message.error(`执行失败：${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setBusy(false);
    }
  };

  // 归档/恢复/webhook 三个轻量操作共用同一套 loading + 错误处理。
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

  const menuItems = buildMenuItems(item, isArchived, runMutation);

  // 主操作按钮随分类切换：归档→恢复，其余→执行一次
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
  ) : (
    <Button
      type="primary"
      size="small"
      icon={<PlayCircleOutlined />}
      loading={busy}
      onClick={(e) => {
        e.stopPropagation();
        handleExecute();
      }}
    >
      执行一次
    </Button>
  );

  return (
    <div
      className="todo-center-card"
      // React Portal 会把 Dropdown 菜单的合成事件冒泡回卡片（即便菜单 DOM 在 body），
      // 因此不能只靠按钮 stopPropagation——点菜单项仍会触发卡片跳详情。
      // 这里用 target 检测：点击源自按钮/下拉/弹窗时视为操作意图，不跳详情。
      onClick={(e) => {
        if ((e.target as HTMLElement).closest('button, .ant-dropdown, .ant-modal')) return;
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
        <Dropdown
          menu={{ items: menuItems }}
          trigger={['click']}
          placement="bottomRight"
        >
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
        <Tag color={BUCKET_DISPLAY[item.computed_bucket].color}>
          {BUCKET_DISPLAY[item.computed_bucket].label}
        </Tag>
        <StatusTag status={item.status} />
        {sourceLabel(item.action_type) && (
          <Tag color="gold">{sourceLabel(item.action_type)}</Tag>
        )}
        {item.webhook_enabled && item.computed_bucket !== 'event_driven' && (
          <Tag color="purple" icon={<ThunderboltOutlined />}>兼事件</Tag>
        )}
        {item.scheduler_config && item.computed_bucket !== 'time_driven' && (
          <Tag color="cyan" icon={<ClockCircleOutlined />}>兼时间</Tag>
        )}
      </div>

      <CardMeta item={item} />

      <div className="todo-center-card-foot">
        <span className="todo-center-card-time">
          {item.archived_at
            ? `归档于 ${formatRelativeTime(item.archived_at)}`
            : `更新于 ${formatRelativeTime(item.updated_at)}`}
        </span>
        {mainAction}
      </div>
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
function CardMeta({ item }: { item: TodoCenterItem }) {
  return (
    <div className="todo-center-card-meta">
      {item.computed_bucket === 'time_driven' && item.scheduler_config && (
        <MetaLine icon={<ClockCircleOutlined />} text={`调度 ${item.scheduler_config}`} />
      )}
      {item.computed_bucket === 'time_driven' && item.scheduler_next_run_at && (
        <MetaLine text={`下次运行 ${formatRelativeTime(item.scheduler_next_run_at)}`} />
      )}
      {item.computed_bucket === 'loop_driven' && (
        <MetaLine icon={<RetweetOutlined />} text={`被 ${item.used_by_loop_step_count} 个启用环节引用`} />
      )}
      {item.last_execution_status && (
        <MetaLine text={`最近执行 ${item.last_execution_status}${item.last_execution_at ? ` · ${formatRelativeTime(item.last_execution_at)}` : ''}`} />
      )}
    </div>
  );
}

/** 单行元信息：可选图标 + 文本，空则不渲染。 */
function MetaLine({ icon, text }: { icon?: React.ReactNode; text: string }) {
  return (
    <div className="todo-center-card-meta-line">
      {icon && <span className="todo-center-card-meta-icon">{icon}</span>}
      <span>{text}</span>
    </div>
  );
}

/**
 * 构建「更多」菜单项：归档/恢复、事件驱动开关。
 *
 * 归档用 `Modal.confirm` 而非 `Popconfirm`：Popconfirm 嵌在 Dropdown 菜单项里时，
 * Dropdown 会在点击时先关闭并卸载菜单，导致 Popconfirm 弹层来不及展开。
 * `Modal.confirm` 在菜单外独立渲染，规避这个时序问题，也更适合放「Loop 引用不受影响」的较长提示。
 */
function buildMenuItems(
  item: TodoCenterItem,
  isArchived: boolean,
  runMutation: (label: string, fn: () => Promise<unknown>) => void,
): MenuProps['items'] {
  const items: NonNullable<MenuProps['items']> = [];

  if (isArchived) {
    items.push({
      key: 'restore',
      label: '恢复事项',
      onClick: () => runMutation('恢复', () => db.restoreTodo(item.id)),
    });
  } else {
    items.push({
      key: 'archive',
      label: '归档',
      onClick: () =>
        Modal.confirm({
          title: '确认归档该事项？',
          content: '归档仅从日常视图隐藏，不删除数据，也不解除 Loop 引用。已归档事项可在「已归档」分类恢复。',
          okText: '归档',
          cancelText: '取消',
          onOk: () => runMutation('归档', () => db.archiveTodo(item.id)),
        }),
    });
    // 事件驱动开关：开启/关闭 webhook，与 scheduler 端点对称
    items.push({
      key: 'webhook',
      label: item.webhook_enabled ? '关闭事件驱动' : '设为事件驱动',
      onClick: () =>
        runMutation(
          item.webhook_enabled ? '关闭事件驱动' : '设为事件驱动',
          () => db.updateTodoWebhook(item.id, !item.webhook_enabled),
        ),
    });
  }

  return items;
}
