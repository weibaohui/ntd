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
// 5. **节省空间**：仅渲染三按钮（checkbox + 批量 + 新建），不显示「已选 N 项 / 共 N 项」文案，
//    选中数 / 总数靠全选复选框三态视觉隐式表达。
// 6. **拆分内部子组件**：SelectAll / BatchDropdown / CreateButton 各自 < 30 行，
//    方便后续单测 + 维护。

import { Dropdown, Button } from 'antd';
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
  /** 隐藏新建按钮（比如把新建按钮放到页面标题栏时） */
  hideCreate?: boolean;

  // —— 批量菜单（差异点 #2：每种模式不同） ——
  batchActions: BatchActionItem<TId>[];
  /** 批量按钮的 trigger 文案，默认 "批量"（2 字紧凑）；菜单项 label 由 batchActions 决定。 */
  batchLabel?: string;

  // —— 可选展示 ——
  /** 整体隐藏（如列表为空）。 */
  hidden?: boolean;
  className?: string;
}

// ─── 内部子组件：SelectDropdown ─────────────────────────────────

interface SelectDropdownProps<TId extends Key> {
  selectableIds: TId[];
  selectedIds: TId[];
  onChange: (next: TId[]) => void;
}

function SelectDropdown<TId extends Key>({ selectableIds, selectedIds, onChange }: SelectDropdownProps<TId>) {
  // 计算状态
  const allCount = selectableIds.length;
  const selectedSet = new Set<TId>(selectedIds);
  const isAll = allCount > 0 && selectableIds.every(id => selectedSet.has(id));
  const isPartial = !isAll && selectedIds.some(id => selectedSet.has(id));
  const unselectedCount = selectableIds.filter(id => !selectedSet.has(id)).length;

  // 全选动作
  const handleSelectAll = () => {
    onChange([...selectableIds]);
  };

  // 反选动作
  const handleInvert = () => {
    const inverted: TId[] = [];
    for (const id of selectableIds) {
      if (!selectedSet.has(id)) {
        inverted.push(id);
      }
    }
    onChange(inverted);
  };

  // 下拉菜单项
  const items: MenuProps['items'] = [
    {
      key: 'all',
      label: '全选',
      onClick: handleSelectAll,
    },
    {
      key: 'invert',
      label: `反选 (${unselectedCount}/${allCount})`,
      onClick: handleInvert,
    },
  ];

  // 下拉按钮文案：显示当前选中状态
  const dropdownLabel = () => {
    if (isAll) return '已全选';
    if (isPartial) return '部分选';
    return '选择';
  };

  return (
    <Dropdown menu={{ items }} trigger={['click']}>
      <Button size="small" data-testid="action-toolbar-select-dropdown">
        {dropdownLabel()} <DownOutlined style={{ fontSize: 10 }} />
      </Button>
    </Dropdown>
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
    createLabel, createIcon, onCreate, hideCreate,
    batchActions, batchLabel = '批量',
    hidden, className,
  } = props;

  if (hidden) return null;

  // 节省空间：用户反馈不再显示「已选 N 项」「共 N 项」文案。
  // 选中数 / 总数由全选复选框的三态视觉（indeterminate / checked）隐式表达。

  return (
    <div
      className={className}
      data-testid="action-toolbar"
      style={{
        display: 'flex',
        alignItems: 'center',
        gap: 8,
        padding: '6px 12px',
        borderBottom: '1px solid var(--color-border-light)',
        background: 'var(--color-bg-elevated, #ffffff)',
        flexShrink: 0,
      }}
    >
      <SelectDropdown
        selectableIds={selectableIds}
        selectedIds={selectedIds}
        onChange={onSelectionChange}
      />
      <div style={{ flex: 1 }} />
      <BatchDropdown
        selectedIds={selectedIds}
        batchActions={batchActions}
        batchLabel={batchLabel}
      />
      {!hideCreate && <CreateButton label={createLabel} icon={createIcon} onClick={onCreate} />}
    </div>
  );
}
