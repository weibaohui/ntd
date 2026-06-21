// 通用列表操作工具栏：用于事项 / 环节 / 环路等任意「可批量管理」列表的头部。
//
// 设计要点：
// 1. **受控组件**：所有选择态由父组件持有，本组件只负责渲染 + 回传变更事件，
//    让三种模式（事项/环节/环路）能各自扩展差异点（createLabel / batchActions）。
// 2. **三态全选**：用 antd `Checkbox` 的 indeterminate 区分「空选 / 部分选 / 全选」，
//    符合用户对批量操作工具栏的常规预期。
// 3. **「全选」语义**：只选当前过滤后的可见项（selectableIds），
//    不去查后端的全量数据 — 过滤后选中是用户能直接感知的范围。
// 4. **批量按钮空选时禁用**：菜单项仍渲染（让用户知道有哪些可做的操作），
//    但 trigger 按钮 disabled，避免误导。
// 5. **拆分内部子组件**：SelectAll / BatchDropdown / CreateButton 各自 < 30 行，
//    方便后续单测 + 维护。

import { Checkbox, Dropdown, Button } from 'antd';
import type { MenuProps } from 'antd';
import { DownOutlined, PlusOutlined } from '@ant-design/icons';
import type { Key, ReactNode } from 'react';

// ─── 类型导出 ────────────────────────────────────────────────

/** 单个批量操作菜单项。组件不感知业务语义，只负责点击时回传当前已选 id 列表。 */
export interface BatchActionItem<TId extends Key = number> {
  key: string;
  label: string;
  icon?: ReactNode;
  danger?: boolean;
  /** 收到当前 selectedIds，由父组件决定如何执行（弹 Modal / 调 API 等）。 */
  onClick: (selectedIds: TId[]) => void;
}

export interface ActionToolbarProps<TId extends Key = number> {
  // —— 选择态 ——
  /** 当前过滤后可见、可选的 id 列表。点「全选」会全选这批。 */
  selectableIds: TId[];
  /** 已选 id 列表（受控）。 */
  selectedIds: TId[];
  /** 选择变化回传。组件只透传，逻辑由父组件决定（合并 / 替换 / 移除）。 */
  onSelectionChange: (next: TId[]) => void;

  // —— 新建按钮（差异点 #1：每种模式不同） ——
  createLabel: string;
  createIcon?: ReactNode;
  onCreate?: () => void;

  // —— 批量菜单（差异点 #2：每种模式不同） ——
  batchActions: BatchActionItem<TId>[];
  /** 批量按钮的 trigger 文案，默认 "批量"（2 字紧凑）；菜单项 label 由 batchActions 决定。 */
  batchLabel?: string;

  // —— 可选展示 ——
  /** 总数（仅用于显示 "共 N 项"），无过滤时与 selectableIds.length 相等。 */
  totalCount?: number;
  /** 整体隐藏（如列表为空）。 */
  hidden?: boolean;
  className?: string;
}

// ─── 内部子组件：SelectAll ───────────────────────────────────

interface SelectAllProps<TId extends Key> {
  selectableIds: TId[];
  selectedIds: TId[];
  onChange: (next: TId[]) => void;
}

function SelectAll<TId extends Key>({ selectableIds, selectedIds, onChange }: SelectAllProps<TId>) {
  // 三态判定：与 antd `Checkbox` 的 indeterminate 协议对齐
  // indeterminate 时 input.checked 为 false，aria-checked='mixed' 反映部分选
  const allCount = selectableIds.length;
  const selectedSet = new Set<TId>(selectedIds);
  const isAll = allCount > 0 && selectableIds.every(id => selectedSet.has(id));
  const isPartial = !isAll && selectedIds.some(id => selectedSet.has(id));

  // 点击行为：未全选 → 全选；已全选 → 清空
  const handleToggle = () => {
    if (isAll || isPartial) {
      onChange([]); // 任何非空状态点击都清空
    } else {
      onChange([...selectableIds]); // 全选当前可见项
    }
  };

  return (
    <Checkbox
      checked={isAll}
      indeterminate={isPartial}
      onChange={handleToggle}
      data-testid="action-toolbar-select-all"
    >
      全选
    </Checkbox>
  );
}

// ─── 内部子组件：BatchDropdown ──────────────────────────────

interface BatchDropdownProps<TId extends Key> {
  selectedIds: TId[];
  batchActions: BatchActionItem<TId>[];
  batchLabel: string;
}

function BatchDropdown<TId extends Key>({ selectedIds, batchActions, batchLabel }: BatchDropdownProps<TId>) {
  // 空选时禁用 trigger，但菜单项照常渲染（让用户能预览能力）
  const disabled = selectedIds.length === 0;

  // 把 BatchActionItem 数组翻译成 antd MenuProps.items
  const items: MenuProps['items'] = batchActions.map(action => ({
    key: action.key,
    label: action.label,
    icon: action.icon,
    danger: action.danger,
    // antd 的 Menu onClick 只回传 key，selectedIds 需闭包捕获
    onClick: () => action.onClick(selectedIds),
    disabled: disabled,
  }));

  return (
    <Dropdown
      menu={{ items }}
      trigger={['click']}
      disabled={disabled}
    >
      <Button
        size="small"
        data-testid="action-toolbar-batch-trigger"
        disabled={disabled}
      >
        {batchLabel} <DownOutlined style={{ fontSize: 10 }} />
      </Button>
    </Dropdown>
  );
}

// ─── 内部子组件：CreateButton ────────────────────────────────

interface CreateButtonProps {
  label: string;
  icon?: ReactNode;
  onClick?: () => void;
}

function CreateButton({ label, icon, onClick }: CreateButtonProps) {
  if (!onClick) return null; // 父组件未提供时不渲染
  return (
    <Button
      type="primary"
      size="small"
      icon={icon ?? <PlusOutlined />}
      onClick={onClick}
      data-testid="action-toolbar-create"
    >
      {label}
    </Button>
  );
}

// ─── 主组件 ────────────────────────────────────────────────

export function ActionToolbar<TId extends Key = number>(props: ActionToolbarProps<TId>) {
  const {
    selectableIds, selectedIds, onSelectionChange,
    createLabel, createIcon, onCreate,
    batchActions, batchLabel = '批量',
    totalCount, hidden, className,
  } = props;

  if (hidden) return null;

  // "已选 N 项" 提示：有选择时显示，无选择时隐藏
  const showSelectedCount = selectedIds.length > 0;
  // "共 N 项" 总数提示：totalCount 与 selectableIds 长度不同时（如有搜索过滤）显示
  const showTotal = totalCount !== undefined && totalCount !== selectableIds.length;

  return (
    <div
      className={className}
      data-testid="action-toolbar"
      style={{
        display: 'flex',
        alignItems: 'center',
        gap: 12,
        padding: '8px 16px',
        borderBottom: '1px solid var(--color-border-light)',
        background: 'var(--color-bg-elevated, #ffffff)',
        flexShrink: 0,
      }}
    >
      <SelectAll
        selectableIds={selectableIds}
        selectedIds={selectedIds}
        onChange={onSelectionChange}
      />
      {showSelectedCount && (
        <span
          data-testid="action-toolbar-selected-count"
          style={{ fontSize: 12, color: 'var(--color-primary)' }}
        >
          已选 {selectedIds.length} 项
        </span>
      )}
      {showTotal && (
        <span style={{ fontSize: 12, color: 'var(--color-text-tertiary)' }}>
          共 {totalCount} 项
        </span>
      )}
      <div style={{ flex: 1 }} />
      <BatchDropdown
        selectedIds={selectedIds}
        batchActions={batchActions}
        batchLabel={batchLabel}
      />
      <CreateButton label={createLabel} icon={createIcon} onClick={onCreate} />
    </div>
  );
}
