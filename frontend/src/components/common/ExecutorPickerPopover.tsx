import { memo, useCallback } from 'react';
import { Dropdown } from 'antd';
import { CheckOutlined } from '@ant-design/icons';
import { EXECUTORS_FOR_PICKER, getExecutorOption } from '@/types';
import type { MenuProps } from 'antd';

interface ExecutorPickerPopoverProps {
  value?: string;
  onChange?: (value: string) => void;
}

/**
 * 执行器选择弹出面板
 *
 * 使用 Ant Design Dropdown 组件，内置边界自动检测：
 * 当下方空间不足时自动向上弹出，避免底部选项被遮挡。
 * 触发按钮保持原有样式（执行器图标+名称+颜色主题）。
 */
export const ExecutorPickerPopover = memo(function ExecutorPickerPopover({
  value = 'claudecode',
  onChange,
}: ExecutorPickerPopoverProps) {
  const current = getExecutorOption(value);

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
          border: `1px solid ${current.color}40`,
          background: `${current.color}10`,
          cursor: 'pointer',
          transition: 'all 0.2s',
        }}
        onMouseEnter={(e) => {
          e.currentTarget.style.borderColor = `${current.color}80`;
        }}
        onMouseLeave={(e) => {
          e.currentTarget.style.borderColor = `${current.color}40`;
        }}
      >
        <span style={{ fontSize: 12, lineHeight: 1 }}>{current.icon}</span>
        <span style={{ fontSize: 13, fontWeight: 600, color: current.color }}>
          {current.label}
        </span>
      </button>
    </Dropdown>
  );
});
