import CountUp from 'react-countup';

interface AnimatedNumberProps {
  value: number;
  duration?: number;
  className?: string;
  style?: React.CSSProperties;
  prefix?: string;
  suffix?: string;
  decimals?: number;
  chineseFormat?: boolean;
}

function getChineseUnit(num: number): { displayValue: number; unit: string; decimals: number } {
  const abs = Math.abs(num);
  if (abs >= 100000000) {
    return { displayValue: num / 100000000, unit: '亿', decimals: 2 };
  }
  if (abs >= 10000) {
    return { displayValue: num / 10000, unit: '万', decimals: 2 };
  }
  return { displayValue: num, unit: '', decimals: 0 };
}

export function AnimatedNumber({
  value,
  duration = 1.2,
  className,
  style,
  prefix,
  suffix,
  decimals = 0,
  chineseFormat = false,
}: AnimatedNumberProps) {
  // Handle null/undefined/NaN values
  const safeValue = value ?? 0;
  if (chineseFormat) {
    const { displayValue, unit, decimals: d } = getChineseUnit(safeValue);
    return (
      <span className={className} style={style}>
        {prefix}
        <CountUp end={displayValue} duration={duration} decimals={d} separator="," />
        {unit}
        {suffix}
      </span>
    );
  }

  return (
    <span className={className} style={style}>
      {prefix}
      <CountUp end={safeValue} duration={duration} decimals={decimals} separator="," />
      {suffix}
    </span>
  );
}
