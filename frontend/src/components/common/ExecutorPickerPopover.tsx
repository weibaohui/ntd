import { memo, useState, useRef, useEffect, useCallback } from 'react';
import { CheckOutlined } from '@ant-design/icons';
import { EXECUTORS_FOR_PICKER, getExecutorOption } from '@/types';

interface ExecutorPickerPopoverProps {
  value: string;
  onChange: (value: string) => void;
}

// 执行器选择弹出面板
// 设计：小按钮显示当前执行器，点击弹出选择面板，选择后关闭
export const ExecutorPickerPopover = memo(function ExecutorPickerPopover({
  value,
  onChange,
}: ExecutorPickerPopoverProps) {
  const [open, setOpen] = useState(false);
  const popoverRef = useRef<HTMLDivElement>(null);
  const current = getExecutorOption(value);

  // 点击外部关闭
  useEffect(() => {
    if (!open) return;

    const handleClickOutside = (e: MouseEvent) => {
      if (popoverRef.current && !popoverRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };

    document.addEventListener('mousedown', handleClickOutside);
    return () => document.removeEventListener('mousedown', handleClickOutside);
  }, [open]);

  const handleSelect = useCallback((v: string) => {
    onChange(v);
    setOpen(false);
  }, [onChange]);

  return (
    <div ref={popoverRef} style={{ position: 'relative', display: 'inline-block' }}>
      {/* 触发按钮 */}
      <button
        onClick={() => setOpen(!open)}
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

      {/* 弹出选择面板 */}
      {open && (
        <div
          style={{
            position: 'absolute',
            top: '100%',
            left: 0,
            marginTop: 4,
            padding: 8,
            borderRadius: 10,
            border: '1px solid var(--color-border-secondary)',
            background: 'var(--color-bg-elevated)',
            boxShadow: '0 4px 12px rgba(0,0,0,0.15)',
            zIndex: 1000,
            minWidth: 200,
            maxHeight: 320,
            overflowY: 'auto',
          }}
        >
          <div style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
            {EXECUTORS_FOR_PICKER.map((opt) => {
              const selected = value === opt.value;
              return (
                <button
                  key={opt.value}
                  onClick={() => handleSelect(opt.value)}
                  style={{
                    display: 'flex',
                    alignItems: 'center',
                    gap: 8,
                    padding: '8px 10px',
                    borderRadius: 6,
                    border: 'none',
                    background: selected ? `${opt.color}15` : 'transparent',
                    cursor: 'pointer',
                    transition: 'all 0.15s',
                    textAlign: 'left',
                  }}
                  onMouseEnter={(e) => {
                    if (!selected) {
                      e.currentTarget.style.background = `${opt.color}08`;
                    }
                  }}
                  onMouseLeave={(e) => {
                    if (!selected) {
                      e.currentTarget.style.background = 'transparent';
                    }
                  }}
                >
                  <span style={{ fontSize: 14, lineHeight: 1 }}>{opt.icon}</span>
                  <span style={{
                    flex: 1,
                    fontSize: 13,
                    fontWeight: 600,
                    color: selected ? opt.color : 'var(--color-text)',
                  }}>
                    {opt.label}
                  </span>
                  {selected && (
                    <span style={{
                      width: 16,
                      height: 16,
                      borderRadius: '50%',
                      backgroundColor: opt.color,
                      display: 'flex',
                      alignItems: 'center',
                      justifyContent: 'center',
                    }}>
                      <CheckOutlined style={{ fontSize: 10, color: '#fff' }} />
                    </span>
                  )}
                </button>
              );
            })}
          </div>
        </div>
      )}
    </div>
  );
});
