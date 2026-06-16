/**
 * 格式化工具函数
 *
 * 集中管理项目中重复出现的格式化逻辑，避免在多个组件中维护各自的实现。
 *
 * 设计权衡：
 * - 统一使用 zh-CN locale：用户群体以中文为主，日期时间格式保持中文习惯。
 * - formatDuration 使用简洁的 "1.5m" 格式：节省 RunningRecordDrawer 等抽屉的 UI
 *   空间，详情页使用 formatDurationSec 提供完整 "1h30m" 格式。
 * - M 值保留 1 位小数：在精度和可读性之间平衡，2 位小数对 token 量级意义不大。
 */

/**
 * 格式化时长（毫秒）为人类可读字符串。
 *
 * 规则：
 * - < 1s → 显示毫秒（如 "500ms"）
 * - < 60s → 显示秒（如 "30s"）
 * - >= 60s → 显示分钟（如 "1.5m"）
 */
export function formatDuration(ms: number | null): string {
  if (ms === null) return '-';
  if (ms < 1_000) return `${ms}ms`;
  if (ms < 60_000) return `${(ms / 1_000).toFixed(0)}s`;
  return `${(ms / 60_000).toFixed(1)}m`;
}

/**
 * 格式化时长（秒）为人类可读字符串。
 *
 * 用于需要先将毫秒转换为秒再格式化的场景。
 * 例如：formatDurationSec(record.usage.duration_ms / 1000)
 */
export function formatDurationSec(seconds: number): string {
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  if (h > 0) return `${h}h${m}m`;
  if (m > 0) return `${m}m`;
  return `${seconds}s`;
}

/**
 * 格式化 token 数量为人类可读字符串。
 *
 * 规则：
 * - < 1000 → 原样显示
 * - < 1M → 显示为 K（如 "1.5K"）
 * - >= 1M → 显示为 M（如 "2.5M"）
 */
export function formatTokens(n: number): string {
  if (n < 1_000) return String(n);
  if (n < 1_000_000) return `${(n / 1_000).toFixed(1)}K`;
  return `${(n / 1_000_000).toFixed(1)}M`;
}

/**
 * 格式化 ISO 时间戳为相对时间字符串（如 "3 分钟前"）。
 *
 * 规则：
 * - < 1 分钟 → "刚刚"
 * - < 60 分钟 → "X 分钟前"
 * - < 24 小时 → "X 小时前"
 * - < 30 天 → "X 天前"
 * - 其他 → 显示日期时间
 */
export function formatRelativeTimeFromNow(iso?: string | null): string {
  if (!iso) return '-';
  try {
    const d = new Date(iso);
    if (Number.isNaN(d.getTime())) return '-';
    const now = new Date();
    const diffMs = now.getTime() - d.getTime();
    const diffMin = Math.floor(diffMs / 60000);

    if (diffMin < 1) return '刚刚';
    if (diffMin < 60) return `${diffMin} 分钟前`;
    const diffHour = Math.floor(diffMin / 60);
    if (diffHour < 24) return `${diffHour} 小时前`;
    const diffDay = Math.floor(diffHour / 24);
    if (diffDay < 30) return `${diffDay} 天前`;

    return d.toLocaleDateString('zh-CN', {
      month: '2-digit',
      day: '2-digit',
      hour: '2-digit',
      minute: '2-digit',
    });
  } catch {
    return iso;
  }
}

/**
 * 格式化 ISO 时间戳为完整日期时间字符串（如 "2024-01-15 14:30"）。
 */
export function formatDateTime(iso: string | null): string {
  if (!iso) return '-';
  try {
    const d = new Date(iso);
    if (Number.isNaN(d.getTime())) return iso;
    return (
      d.toLocaleDateString('zh-CN') +
      ' ' +
      d.toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' })
    );
  } catch {
    return iso;
  }
}

/**
 * 格式化 ISO 时间戳为时分秒字符串（如 "14:30:25"）。
 */
export function formatTimeFull(iso?: string): string {
  if (!iso) return '';
  try {
    const d = new Date(iso);
    if (Number.isNaN(d.getTime())) return '';
    return d.toLocaleTimeString('zh-CN', {
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit',
      hour12: false,
    });
  } catch {
    return '';
  }
}

/**
 * 计算从指定时间到现在经过的秒数。
 */
export function elapsedSeconds(startTimeStr: string | null | undefined): number {
  const date = new Date(startTimeStr || '');
  if (Number.isNaN(date.getTime())) return 0;
  const now = new Date();
  return Math.floor((now.getTime() - date.getTime()) / 1000);
}

/**
 * 格式化文件大小为人类可读字符串。
 *
 * 规则：
 * - < 1KB → 显示字节（如 "500 B"）
 * - < 1MB → 显示 KB（如 "1.5 KB"）
 * - < 1GB → 显示 M（如 "2.5 M"），不带 B 后缀，节省 UI 空间
 * - >= 1GB → 显示 G（如 "1.2 G"），不带 B 后缀，节省 UI 空间
 *
 * 设计权衡：
 * - M/G 不带 B 是为了节省设置-备份列表的横向空间，避免长单位撑开 List 行；
 *   在 M/G 量级下单位语义已足够清晰，B 后缀是冗余的。
 * - 仍使用 1024 进制（KB/MB/GB 二进制语义），与文件管理器习惯一致。
 */
export function formatFileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} M`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} G`;
}
