/**
 * 解析后端返回的时间字符串为 Date 对象。
 * 后端存储的是本地时间（已按当前时区格式化），不加 Z 直接解析为本地时间。
 * 如果后端返回的是 UTC 时间（带 Z），则按 UTC 解析。
 */
export function parseUtcDate(timeStr: string | null | undefined): Date | null {
  if (!timeStr) return null;
  // 有 Z 后缀说明后端认为这是 UTC 时间，按 UTC 解析
  if (timeStr.endsWith('Z')) return new Date(timeStr);
  // 没有 Z 后缀说明后端存的是本地时间，直接解析
  return new Date(timeStr);
}

/**
 * 将 UTC 时间字符串格式化为本地时区的可读字符串
 */
export function formatLocalDateTime(timeStr: string | null | undefined): string {
  const date = parseUtcDate(timeStr);
  if (!date) return '';
  return date.toLocaleString();
}

/**
 * 将时间格式化为相对时间（多久之前）。
 * 使用 UTC 计算经过的时分秒，避免本地时区偏移导致显示错误（如"3小时前"变成"11小时前"）。
 */
export function formatRelativeTime(timeStr: string | null | undefined): string {
  const date = parseUtcDate(timeStr);
  if (!date) return '';

  const now = new Date();
  const diffMs = now.getTime() - date.getTime();
  if (diffMs < 0) return '';

  const diffSec = Math.floor(diffMs / 1000);
  const diffMin = Math.floor(diffSec / 60);
  const diffHour = Math.floor(diffMin / 60);
  const diffDay = Math.floor(diffHour / 24);

  if (diffSec < 10) return '刚刚';
  if (diffSec < 60) return `${diffSec} 秒前`;
  if (diffMin < 60) return `${diffMin} 分钟前`;
  if (diffHour < 24) return `${diffHour} 小时前`;
  if (diffDay === 1) return '昨天';
  if (diffDay < 7) return `${diffDay} 天前`;

  // 超过7天显示月日，用 UTC 避免本地时区偏移
  return date.toLocaleDateString('zh-CN', {
    timeZone: 'UTC',
    month: 'numeric',
    day: 'numeric',
  });
}

/**
 * 格式化时长（秒）为简写形式，如 1h30m, 3m10s, 45s
 */
export function formatDuration(seconds: number): string {
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  if (h > 0) return `${h}h${m}m`;
  if (m > 0) return `${m}m`;
  return `${seconds}s`;
}

/**
 * 计算从指定时间到现在经过的秒数
 */
export function elapsedSeconds(startTimeStr: string | null | undefined): number {
  const date = parseUtcDate(startTimeStr);
  if (!date) return 0;
  const now = new Date();
  return Math.floor((now.getTime() - date.getTime()) / 1000);
}
