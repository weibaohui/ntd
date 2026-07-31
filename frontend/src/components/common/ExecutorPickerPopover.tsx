import { memo, useCallback } from 'react';
import { Dropdown } from 'antd';
import { CheckOutlined, CloseOutlined } from '@ant-design/icons';
import { EXECUTORS_FOR_PICKER, getExecutorOption } from '@/types';
import type { MenuProps } from 'antd';

interface ExecutorPickerPopoverProps {
  value?: string;
  onChange?: (value: string) => void;
  /** 传入后，有值时按钮内显示清空按钮；点清空不弹下拉，仅回调 onClear。 */
  onClear?: () => void;
  /** 无值时的占位文案。 */
  placeholder?: string;
}

/**
 * 执行器选择弹出面板
 *
 * 使用 Ant Design Dropdown 组件，内置边界自动检测：
 * 当下方空间不足时自动向上弹出，避免底部选项被遮挡。
 * 触发按钮保持原有样式（执行器图标+名称+颜色主题）。
 *
 * 需求 053 扩展：value 为空时不回退默认 claudecode，而是显示 placeholder（避免"看似已选实则为空"的误导）；
 * 传 onClear 时有值显示清空按钮。既有调用方（闪念/动作/聊天）均传有值且不传 onClear，行为不变。
 */
export const ExecutorPickerPopover = memo(function ExecutorPickerPopover({
  value,
  onChange,
  onClear,
  placeholder = '未选择执行器',
}: ExecutorPickerPopoverProps) {
  // 空值显示占位：不再默认回退 claudecode，让"未选择"在 UI 上可见
  const current = value ? getExecutorOption(value) : null;
  const color = current?.color ?? 'var(--color-text-tertiary)';

  // 构建下拉菜单项
  const items: MenuProps['items'] = EXECUTORS_FOR_PICKER.map((opt) => ({
    key: opt.value,
    label: (
      <span style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
        <span style={{ fontSize: 14, lineHeight: 1 }}>{opt.icon}</span>
        <span style={{
          flex: 1,
          fontSize: 13,
          fontWeight: 600,
          color: value === opt.value ? opt.color : 'var(--color-text)',
        }}>
          {opt.label}
        </span>
        {value === opt.value && (
          <CheckOutlined style={{ fontSize: 12, color: opt.color }} />
        )}
      </span>
    ),
  }));

  const handleMenuClick = useCallback<NonNullable<MenuProps['onClick']>>(({ key }) => {
    onChange?.(String(key));
  }, [onChange]);

  return (
    <Dropdown
      menu={{ items, onClick: handleMenuClick }}
      // bottomLeft 优先；Ant Design 内置自动边界检测，下方空间不足时自动翻转到 topLeft
      placement="bottomLeft"
      // 触发方式：点击
      trigger={['click']}
    >
      <button
        style={{
          display: 'flex',
          alignItems: 'center',
          gap: 6,
          padding: '6px 12px',
          borderRadius: 8,
          border: `1px solid ${color}40`,
          background: `${color}10`,
          cursor: 'pointer',
          transition: 'all 0.2s',
        }}
        onMouseEnter={(e) => {
          e.currentTarget.style.borderColor = `${color}80`;
        }}
        onMouseLeave={(e) => {
          e.currentTarget.style.borderColor = `${color}40`;
        }}
      >
        {current && <span style={{ fontSize: 12, lineHeight: 1 }}>{current.icon}</span>}
        <span style={{ fontSize: 13, fontWeight: 600, color: current?.color ?? color }}>
          {current?.label ?? placeholder}
        </span>
        {/* 清空按钮：仅在有值且传 onClear 时显示；stopPropagation 避免点击它触发下拉 */}
        {value && onClear && (
          <span
            role="button"
            aria-label="清空执行器"
            onClick={(e) => {
              e.stopPropagation();
              onClear();
            }}
            style={{
              display: 'inline-flex',
              fontSize: 10,
              color: 'var(--color-text-tertiary)',
              cursor: 'pointer',
              marginLeft: 2,
            }}
          >
            <CloseOutlined />
          </span>
        )}
      </button>
    </Dropdown>
  );
});
