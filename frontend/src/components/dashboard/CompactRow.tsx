import type { ReactNode } from 'react';

export function CompactRow({ name, value, sub, color, barPct }: {
  name: string; value: ReactNode; sub: ReactNode; color: string; barPct: number;
}) {
  return (
    <div style={{ padding: '10px 0', borderBottom: '1px solid var(--color-border-secondary)' }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'baseline', marginBottom: 6 }}>
        <span style={{ fontSize: 13, fontWeight: 600, color: 'var(--color-text)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap', marginRight: 12 }} title={name}>{name}</span>
        {value}
      </div>
      <div style={{ height: 4, borderRadius: 2, background: 'var(--color-fill-quaternary)', marginBottom: 6 }}>
        <div style={{ height: '100%', width: `${Math.max(barPct, 0)}%`, minWidth: barPct > 0 ? 4 : 0, borderRadius: 2, background: color, transition: 'width 0.6s ease' }} />
      </div>
      <div style={{ fontSize: 11, color: 'var(--color-text-tertiary)' }}>{sub}</div>
    </div>
  );
}
