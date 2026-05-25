import { useState } from 'react';
import type { TodoItem } from '../../types';

/** 任务进度展示组件，显示子项完成情况 */
export function ProgressWidget({ items }: { items: TodoItem[] }) {
  const [expanded, setExpanded] = useState(false);
  const total = items.length;
  const completed = items.filter(t => t.status === 'completed').length;
  const pct = Math.round((completed / total) * 100);

  return (
    <div style={{ position: 'relative', flexShrink: 0 }}>
      <div
        onClick={() => setExpanded(!expanded)}
        style={{
          background: 'var(--color-bg-elevated)',
          borderRadius: 6,
          padding: '4px 10px',
          border: `1px solid ${expanded ? 'var(--color-primary)' : 'var(--color-border-light)'}`,
          minWidth: 120,
          cursor: 'pointer',
          userSelect: 'none',
          transition: 'border-color 0.2s',
        }}
      >
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 3 }}>
          <span style={{ fontSize: 10, fontWeight: 600, color: 'var(--color-text-secondary)' }}>进度</span>
          <span style={{ fontSize: 10, color: 'var(--color-primary)', fontWeight: 600 }}>{completed}/{total} ({pct}%)</span>
        </div>
        <div style={{ height: 3, borderRadius: 2, background: 'var(--color-border-light)', marginBottom: 3 }}>
          <div style={{ height: '100%', borderRadius: 2, background: 'var(--color-primary)', width: `${pct}%`, transition: 'width 0.3s' }} />
        </div>
        <div style={{ display: 'flex', gap: 3, flexWrap: 'wrap' }}>
          {items.map((item, idx) => (
            <span key={item.id || idx} style={{ fontSize: 10, lineHeight: '14px', color: item.status === 'completed' ? 'var(--color-text-tertiary)' : item.status === 'in_progress' ? 'var(--color-primary)' : 'var(--color-text-secondary)', textDecoration: item.status === 'completed' ? 'line-through' : 'none', maxWidth: 80, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
              {item.status === 'completed' ? '✓' : item.status === 'in_progress' ? '●' : '○'} {item.content}
            </span>
          ))}
        </div>
      </div>
      {expanded && (
        <div style={{
          position: 'absolute',
          top: '100%',
          right: 0,
          zIndex: 20,
          marginTop: 4,
          background: 'var(--color-bg-elevated)',
          border: '1px solid var(--color-border-light)',
          borderRadius: 8,
          padding: 12,
          boxShadow: '0 6px 20px rgba(0,0,0,0.15)',
          minWidth: 260,
          maxWidth: 360,
        }}>
          <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 8 }}>
            <span style={{ fontSize: 12, fontWeight: 700, color: 'var(--color-text)' }}>任务进度</span>
            <span style={{ fontSize: 11, color: 'var(--color-primary)', fontWeight: 600 }}>{completed}/{total} ({pct}%)</span>
          </div>
          <div style={{ height: 4, borderRadius: 2, background: 'var(--color-border-light)', marginBottom: 10 }}>
            <div style={{ height: '100%', borderRadius: 2, background: 'var(--color-primary)', width: `${pct}%`, transition: 'width 0.3s' }} />
          </div>
          <div style={{ display: 'flex', flexDirection: 'column', gap: 6, maxHeight: 280, overflow: 'auto' }}>
            {items.map((item, idx) => (
              <div key={item.id || idx} style={{
                display: 'flex',
                alignItems: 'flex-start',
                gap: 8,
                fontSize: 12,
                lineHeight: '18px',
                color: item.status === 'completed' ? 'var(--color-text-tertiary)' : item.status === 'in_progress' ? 'var(--color-primary)' : 'var(--color-text-secondary)',
                textDecoration: item.status === 'completed' ? 'line-through' : 'none',
                padding: '4px 8px',
                borderRadius: 4,
                background: item.status === 'in_progress' ? 'var(--color-primary-bg)' : 'transparent',
              }}>
                <span style={{ flexShrink: 0, marginTop: 2 }}>
                  {item.status === 'completed' ? '✓' : item.status === 'in_progress' ? '●' : '○'}
                </span>
                <span style={{ wordBreak: 'break-word' }}>{item.content}</span>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
