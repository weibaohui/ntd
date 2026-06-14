import { memo } from 'react';
import { CheckOutlined } from '@ant-design/icons';
import type { ExecutorOption } from '@/types';

export const ExecutorPicker = memo(function ExecutorPicker({ executor, executorOptions, onChange }: {
  executor: string;
  executorOptions: ExecutorOption[];
  onChange: (v: string) => void;
}) {
  return (
    <div style={{ marginBottom: 16 }}>
      <div style={{ marginBottom: 10, fontWeight: 600, fontSize: 14 }}>执行器</div>
      <div style={{ display: 'flex', flexWrap: 'wrap', gap: 10 }}>
        {executorOptions.map((opt) => {
          const selected = executor === opt.value;
          return (
            <div
              key={opt.value}
              onClick={() => onChange(opt.value)}
              role="button"
              tabIndex={0}
              onKeyDown={(e) => {
                if (e.key === 'Enter' || e.key === ' ') {
                  e.preventDefault();
                  onChange(opt.value);
                }
              }}
              style={{
                display: 'flex',
                alignItems: 'center',
                gap: 8,
                padding: '10px 14px',
                borderRadius: 10,
                border: `2px solid ${selected ? opt.color : 'var(--color-border-secondary)'}`,
                background: selected ? `${opt.color}10` : 'var(--color-bg-elevated)',
                cursor: 'pointer',
                transition: 'all 0.2s ease',
                flex: '1 1 calc(50% - 10px)',
                minWidth: 120,
              }}
              onMouseEnter={(e) => {
                if (!selected) {
                  (e.currentTarget as HTMLDivElement).style.borderColor = `${opt.color}60`;
                  (e.currentTarget as HTMLDivElement).style.background = `${opt.color}08`;
                }
              }}
              onMouseLeave={(e) => {
                if (!selected) {
                  (e.currentTarget as HTMLDivElement).style.borderColor = 'var(--color-border-secondary)';
                  (e.currentTarget as HTMLDivElement).style.background = 'var(--color-bg-elevated)';
                }
              }}
            >
              <span style={{ fontSize: 16, lineHeight: 1 }}>{opt.icon}</span>
              <span style={{
                fontSize: 14,
                fontWeight: 600,
                color: selected ? opt.color : 'var(--color-text)',
                flex: 1,
              }}>
                {opt.label}
              </span>
              {selected && (
                <span style={{
                  width: 18,
                  height: 18,
                  borderRadius: '50%',
                  backgroundColor: opt.color,
                  display: 'flex',
                  alignItems: 'center',
                  justifyContent: 'center',
                  flexShrink: 0,
                }}>
                  <CheckOutlined style={{ fontSize: 10, color: '#fff' }} />
                </span>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
});
