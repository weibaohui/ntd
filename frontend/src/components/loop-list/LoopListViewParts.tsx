// LoopListViewParts — LoopListView 的拆分子模块（响应 028 PR review 的函数体 ≤30 行规范）。
//
// 拆分原则：把列定义、行操作菜单、状态渲染、批量按钮拆到独立函数/组件，
// 让 LoopListView 主函数仅负责组合，函数体保持简短。
//
// 1. LOOP_STATUS_META：环路状态 → 中文 + 颜色映射
// 2. renderLoopStatusTag：状态 Tag 渲染
// 3. buildRowActions：单行操作菜单构建
// 4. buildColumns：列定义构建（useMemo 包装，避免每次渲染重建）
// 5. BatchButton：批量操作按钮组件

import type { ReactNode } from 'react';
import { Button, Dropdown, Tag } from 'antd';
import type { ColumnsType } from 'antd/es/table';
import type { MenuProps } from 'antd';
// antd Menu 项 onClick 的事件参数类型（含 domEvent），从公开 API 推导避免深层依赖 rc-menu
type MenuInfo = Parameters<NonNullable<MenuProps['onClick']>>[0];
import {
  DeleteOutlined,
  MoreOutlined,
  SettingOutlined,
} from '@ant-design/icons';
import { formatRelativeTime } from '@/utils/datetime';
import { formatProcessText } from '@/utils/processText';
import { makeSorter } from '@/hooks/useResizableColumns';
import type { LoopListItem } from '@/types/loop';

/** 环路状态 → 中文 + 颜色；与 LoopStudioDetailPanel 的状态展示保持一致。 */
const LOOP_STATUS_META: Record<string, { label: string; color: string }> = {
  enabled: { label: '已启用', color: 'success' },
  paused: { label: '已暂停', color: 'default' },
  disabled: { label: '已禁用', color: 'error' },
};

/** 把 status 串映射为 Tag；未知状态原样返回，便于扩展。 */
function renderLoopStatusTag(status?: string | null): ReactNode {
  if (!status) return '-';
  const meta = LOOP_STATUS_META[status];
  if (!meta) return status;
  return <Tag color={meta.color}>{meta.label}</Tag>;
}

interface BuildRowActionsArgs {
  onDelete: (loop: LoopListItem) => void;
  onToggleStatus: (loop: LoopListItem) => void;
}

/**
 * 构建单行操作菜单项：切换状态 / 删除。
 * 044：触发/复制已随手工环路能力下线，唯一执行入口是「创建任务选工艺环路」。
 * 抽成纯函数避免 LoopListView 主函数膨胀。
 *
 * 注意（冒泡陷阱）：Dropdown 菜单经 React Portal 渲染，合成事件会沿
 * React 组件树冒泡回表格行（即便菜单 DOM 挂在 body），只在触发按钮上
 * stopPropagation 挡不住「点菜单项 → 行 onClick → 误跳详情」。
 * 因此每个菜单项的 onClick 必须先 domEvent.stopPropagation()。
 */
export function buildRowActions(
  loop: LoopListItem,
  { onDelete, onToggleStatus }: BuildRowActionsArgs,
) {
  // 包装回调：统一先挡冒泡再执行业务动作，避免每个菜单项重复书写
  const guard = (action: (l: LoopListItem) => void) => (info: MenuInfo) => {
    info.domEvent.stopPropagation();
    action(loop);
  };
  return [
    {
      key: 'toggle-status',
      label: loop.status === 'enabled' ? '暂停' : '启用',
      icon: <SettingOutlined />,
      onClick: guard(onToggleStatus),
    },
    {
      key: 'delete',
      label: '删除',
      icon: <DeleteOutlined />,
      danger: true,
      onClick: guard(onDelete),
    },
  ];
}

/**
 * 计算环路「工艺」列统一文本。
 * 名称优先 display_name（中文名），缺失回退标识名 name；版本取实例化快照；手工环路返回 '-'。
 * 抽成纯函数便于单测，并与事项/任务列表共用同一 formatProcessText 口径。
 */
export function loopProcessText(record: LoopListItem): string {
  const name = record.process_template_display_name ?? record.process_template_name;
  return formatProcessText(record.process_template_id, name, record.process_template_version);
}

interface BuildColumnsArgs {
  onSelectLoop: (id: number) => void;
  onDelete: (loop: LoopListItem) => void;
  onToggleStatus: (loop: LoopListItem) => void;
}

/**
 * 构建 Table 列定义。
 * 抽成 useMemo 包装的函数，避免每次渲染重建列对象。
 */
export function buildColumns({
  onSelectLoop,
  onDelete,
  onToggleStatus,
}: BuildColumnsArgs): ColumnsType<LoopListItem> {
  return [
    {
      title: 'ID',
      dataIndex: 'id',
      width: 70,
      fixed: 'left',
      // 054：可排序列（ID 数值排序）
      sorter: makeSorter<LoopListItem>('id'),
      render: (id: number) => (
        <span style={{ fontFamily: 'monospace', color: 'var(--color-text-tertiary)' }}>
          #{id}
        </span>
      ),
    },
    {
      title: '名称',
      dataIndex: 'name',
      ellipsis: true,
      // 054：可排序列（名称字符串排序）
      sorter: makeSorter<LoopListItem>('name'),
      render: (name: string, record) => (
        <a
          onClick={(e) => { e.stopPropagation(); onSelectLoop(record.id); }}
          style={{ color: 'var(--color-text)' }}
        >
          {name}
        </a>
      ),
    },
    {
      title: '工艺',
      key: 'process',
      width: 240,
      ellipsis: true,
      // 三列表统一格式：#工艺id-工艺名称-工艺版本；手工环路显示 '-'。
      render: (_v, record) => loopProcessText(record),
    },
    {
      title: '状态',
      dataIndex: 'status',
      width: 100,
      // 054：可排序列（状态枚举字符串排序）
      sorter: makeSorter<LoopListItem>('status'),
      render: renderLoopStatusTag,
    },
    {
      title: '环节',
      dataIndex: 'step_count',
      width: 70,
      align: 'center' as const,
      // 054：可排序列（环节数数值排序）
      sorter: makeSorter<LoopListItem>('step_count'),
    },
    {
      title: '待审批',
      dataIndex: 'pending_approval_count',
      width: 80,
      align: 'center' as const,
      // 054：可排序列（待审批数数值排序）
      sorter: makeSorter<LoopListItem>('pending_approval_count'),
      render: (n: number) => (n > 0 ? <Tag color="warning">{n}</Tag> : '-'),
    },
    {
      title: '最近执行',
      dataIndex: 'last_execution_status',
      width: 100,
      render: (s?: string | null) => s ? <Tag color={s === 'success' ? 'success' : s === 'failed' ? 'error' : 'processing'}>{s}</Tag> : '-',
    },
    {
      title: '执行时间',
      dataIndex: 'last_execution_at',
      width: 130,
      // 054：可排序列（ISO 时间字符串排序）
      sorter: makeSorter<LoopListItem>('last_execution_at'),
      render: (t?: string | null) => t ? formatRelativeTime(t) : '-',
    },
    {
      title: '更新时间',
      dataIndex: 'updated_at',
      width: 130,
      // 054：可排序列（ISO 时间字符串排序）
      sorter: makeSorter<LoopListItem>('updated_at'),
      render: (t: string | null) => t ? formatRelativeTime(t) : '-',
    },
    {
      title: '操作',
      key: 'actions',
      width: 80,
      fixed: 'right' as const,
      render: (_, record) => (
        <Dropdown menu={{ items: buildRowActions(record, { onDelete, onToggleStatus }) }} trigger={['click']}>
          <Button
            size="small"
            type="text"
            icon={<MoreOutlined />}
            onClick={(e) => e.stopPropagation()}
            aria-label="更多操作"
          />
        </Dropdown>
      ),
    },
  ];
}

interface BatchButtonProps {
  selectedIds: number[];
  batchActions: { key: string; label: string; icon?: ReactNode; danger?: boolean; onClick: (ids: number[]) => void }[];
}

/**
 * 批量按钮：受 selectedIds 控制，空选时禁用。
 * 抽为子组件让 LoopListView 主函数保持在 30 行内。
 */
export function BatchButton({
  selectedIds,
  batchActions,
}: BatchButtonProps) {
  if (batchActions.length === 0) return null;
  const disabled = selectedIds.length === 0;
  const items = batchActions.map(action => ({
    key: action.key,
    label: action.label,
    icon: action.icon,
    danger: action.danger,
    onClick: () => action.onClick(selectedIds),
    disabled,
  }));
  return (
    <Dropdown menu={{ items }} trigger={['click']} disabled={disabled}>
      <Button size="small" disabled={disabled} data-testid="loop-list-batch-trigger">
        批量 <MoreOutlined style={{ fontSize: 10 }} />
      </Button>
    </Dropdown>
  );
}

/** 整行点击跳转详情：抽为独立函数便于测试。 */
export function buildOnRow(onSelectLoop: (id: number) => void) {
  return (record: LoopListItem) => ({
    onClick: () => onSelectLoop(record.id),
    style: { cursor: 'pointer' },
  });
}