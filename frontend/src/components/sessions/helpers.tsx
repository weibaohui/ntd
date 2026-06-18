import { Tag } from 'antd';
import { formatTokens, formatRelativeTimeFromNow } from '@/utils/format';
import { EXECUTOR_COLORS } from '@/types/execution';

export const sourceConfig: Record<string, { label: string; color: string }> = {
  'claudecode': { label: 'Claude Code', color: EXECUTOR_COLORS.claudecode },
  'codex': { label: 'Codex', color: EXECUTOR_COLORS.codex },
  'hermes': { label: 'Hermes', color: EXECUTOR_COLORS.hermes },
  'kimi': { label: 'Kimi', color: EXECUTOR_COLORS.kimi },
  'atomcode': { label: 'AtomCode', color: EXECUTOR_COLORS.atomcode },
  'codebuddy': { label: 'CodeBuddy', color: EXECUTOR_COLORS.codebuddy },
  'opencode': { label: 'OpenCode', color: EXECUTOR_COLORS.opencode },
  'mobilecoder': { label: 'MobileCoder', color: EXECUTOR_COLORS.mobilecoder },
  // Issue #673: zhanlu 颜色从 EXECUTOR_COLORS 取，保持与 execution.tsx 单点 SoT。
  'zhanlu': { label: 'Zhanlu', color: EXECUTOR_COLORS.zhanlu },
};

export function sourceTag(source: string) {
  const cfg = sourceConfig[source] || { label: source, color: '#6b7280' };
  return (
    <Tag color={cfg.color} style={{ fontSize: 11, lineHeight: '18px', padding: '0 6px' }}>
      {cfg.label}
    </Tag>
  );
}

export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export { formatTokens, formatRelativeTimeFromNow as formatTime };

export function shortId(id: string): string {
  return id.length > 12 ? `${id.slice(0, 8)}...${id.slice(-4)}` : id;
}
