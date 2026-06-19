// ♾ 无穷环路图标 — 使用 react-icons 的 BiInfinite。
import { BiInfinite } from 'react-icons/bi';

interface LoopIconProps {
  style?: React.CSSProperties;
  className?: string;
}

export function LoopIcon({ style, className }: LoopIconProps) {
  return <BiInfinite style={style} className={className} />;
}
