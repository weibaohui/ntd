import { AnimatedNumber } from '../AnimatedNumber';

interface MiniStatProps {
  title: string;
  value: number;
  suffix?: string;
  prefix?: React.ReactNode;
  color: string;
  loading?: boolean;
  decimals?: number;
  chineseFormat?: boolean;
}

export function MiniStat({ title, value, suffix, prefix, color, loading, decimals = 0, chineseFormat = false }: MiniStatProps) {
  return (
    <div style={{ display: 'flex', alignItems: 'center', gap: 12, padding: '12px 14px', borderRadius: 10, background: 'var(--color-fill-quaternary)', transition: 'background 0.2s' }}>
      <div
        style={{
          width: 40,
          height: 40,
          borderRadius: 10,
          backgroundColor: `${color}18`,
          color,
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center',
          fontSize: 18,
          flexShrink: 0,
        }}
      >
        {prefix}
      </div>
      <div style={{ minWidth: 0 }}>
        <div style={{ fontSize: 12, color: 'var(--color-text-secondary)', marginBottom: 2 }}>{title}</div>
        <div style={{ fontSize: 22, fontWeight: 700, color: 'var(--color-text)', lineHeight: 1.2 }}>
          <AnimatedNumber value={loading ? 0 : value} duration={0.8} decimals={decimals} chineseFormat={chineseFormat} />
          {suffix && <span style={{ fontSize: 13, fontWeight: 500, marginLeft: 2 }}>{suffix}</span>}
        </div>
      </div>
    </div>
  );
}
