import { memo } from 'react';
import { Popover } from 'antd';
import { CheckOutlined } from '@ant-design/icons';
import { useTheme } from '../hooks/useTheme';

// Light theme status config
const lightStatusConfig: Record<string, { color: string; label: string }> = {
  pending: { color: '#94a3b8', label: '待执行' },
  running: { color: '#3b82f6', label: '执行中' },
  completed: { color: '#22c55e', label: '已完成' },
  failed: { color: '#ef4444', label: '失败' },
};

// Dark theme status config - Catppuccin Mocha inspired
const darkStatusConfig: Record<string, { color: string; label: string }> = {
  pending: { color: '#6c7086', label: '待执行' },
  running: { color: '#89b4fa', label: '执行中' },
  completed: { color: '#a6e3a1', label: '已完成' },
  failed: { color: '#f38ba8', label: '失败' },
};

interface StatusPickerProps {
  value: string;
  onChange: (value: string) => void;
  disabled?: boolean;
}

export const StatusPicker = memo(function StatusPicker({ value, onChange, disabled }: StatusPickerProps) {
  const { themeMode } = useTheme();
  const statusConfig = themeMode === 'dark' ? darkStatusConfig : lightStatusConfig;
  const current = statusConfig[value] || statusConfig.pending;

  const handleSelect = (status: string) => {
    if (status !== value) {
      onChange(status);
    }
  };

  const triggerNode = (
    <div
      style={{
        display: 'inline-flex',
        alignItems: 'center',
        justifyContent: 'center',
        width: 28,
        height: 28,
        borderRadius: '50%',
        backgroundColor: current.color,
        cursor: disabled ? 'not-allowed' : 'pointer',
        opacity: disabled ? 0.5 : 1,
        border: 'none',
        flexShrink: 0,
        transition: 'all 0.2s ease',
        boxShadow: `0 2px 6px ${current.color}40`,
      }}
      role="button"
      tabIndex={0}
      aria-label={`当前状态: ${current.label}`}
    />
  );

  if (disabled) {
    return triggerNode;
  }

  return (
    <Popover
      content={
        <div style={{ padding: 2, minWidth: 120 }}>
          {Object.entries(statusConfig).map(([key, config]) => (
            <div
              key={key}
              style={{
                display: 'flex',
                alignItems: 'center',
                gap: 8,
                padding: '6px 10px',
                borderRadius: 6,
                cursor: 'pointer',
                transition: 'background 150ms ease',
                background: value === key ? 'var(--color-primary-bg)' : 'transparent',
              }}
              onClick={() => handleSelect(key)}
              onMouseEnter={(e) => {
                if (value !== key) {
                  e.currentTarget.style.background = 'var(--color-bg-hover)';
                }
              }}
              onMouseLeave={(e) => {
                if (value !== key) {
                  e.currentTarget.style.background = 'transparent';
                }
              }}
            >
              <span
                style={{
                  width: 12,
                  height: 12,
                  borderRadius: '50%',
                  backgroundColor: config.color,
                  flexShrink: 0,
                }}
              />
              <span style={{ fontSize: 13, color: 'var(--color-text)', fontWeight: 500 }}>
                {config.label}
              </span>
              {value === key && <CheckOutlined style={{ color: 'var(--color-primary)', fontWeight: 700, fontSize: 11 }} />}
            </div>
          ))}
        </div>
      }
      trigger="click"
      placement="bottomLeft"
    >
      {triggerNode}
    </Popover>
  );
});
