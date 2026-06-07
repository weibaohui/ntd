import { Tag } from 'antd';
import { parseUtcDate } from '../../utils/datetime';

export const sourceConfig: Record<string, { label: string; color: string }> = {
  'claudecode': { label: 'Claude Code', color: '#d97706' },
  'codex': { label: 'Codex', color: '#10a37f' },
  'hermes': { label: 'Hermes', color: '#8b5cf6' },
  'kimi': { label: 'Kimi', color: '#3b82f6' },
  'atomcode': { label: 'AtomCode', color: '#ef4444' },
  'codebuddy': { label: 'CodeBuddy', color: '#f59e0b' },
  'opencode': { label: 'OpenCode', color: '#22c55e' },
  'joinai': { label: 'JoinAI', color: '#6366f1' },
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

export function formatTokens(n: number): string {
  if (n < 1000) return String(n);
  if (n < 1_000_000) return `${(n / 1000).toFixed(1)}K`;
  return `${(n / 1_000_000).toFixed(2)}M`;
}

export function formatTime(iso?: string | null): string {
  if (!iso) return '-';
  try {
    const d = parseUtcDate(iso);
    if (!d) return '-';
    const now = new Date();
    const diffMs = now.getTime() - d.getTime();
    const diffMin = Math.floor(diffMs / 60000);

    if (diffMin < 1) return '刚刚';
    if (diffMin < 60) return `${diffMin} 分钟前`;
    const diffHour = Math.floor(diffMin / 60);
    if (diffHour < 24) return `${diffHour} 小时前`;
    const diffDay = Math.floor(diffHour / 24);
    if (diffDay < 30) return `${diffDay} 天前`;

    return d.toLocaleDateString('zh-CN', { month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit' });
  } catch {
    return iso;
  }
}

export function shortId(id: string): string {
  return id.length > 12 ? `${id.slice(0, 8)}...${id.slice(-4)}` : id;
}
