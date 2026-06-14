/**
 * 将后端返回的 UTC ISO 8601 时间字符串（带 Z 后缀）解析为 Date 对象。
 * JS 的 Date 构造器会自动将 Z 视为 UTC 时间，
 * 在 toLocaleString() / getTime() 等操作时转换为本地时区。
 */
import { formatDistanceToNow, parseISO } from 'date-fns';
import { zhCN } from 'date-fns/locale';
import { formatDurationSec } from './format';

export function parseUtcDate(timeStr: string | null | undefined): Date | null {
  if (!timeStr) return null;
  return new Date(timeStr);
}

/**
 * 格式化为本地可读字符串
 */
export function formatLocalDateTime(timeStr: string | null | undefined): string {
  const date = parseUtcDate(timeStr);
  if (!date) return '';
  return date.toLocaleString();
}

/**
 * 格式化为相对时间（多久之前），使用 date-fns
 */
export function formatRelativeTime(timeStr: string | null | undefined): string {
  if (!timeStr) return '';
  const date = parseISO(timeStr);
  return formatDistanceToNow(date, { addSuffix: true, locale: zhCN });
}

/**
 * 格式化时长（秒）为人类可读字符串。
 *
 * 委托给 format.ts 中的 formatDurationSec 实现。
 * 保留此导出以维持向后兼容性。
 */
export const formatDuration = formatDurationSec;

/**
 * 计算从指定时间到现在经过的秒数
 */
export function elapsedSeconds(startTimeStr: string | null | undefined): number {
  const date = parseUtcDate(startTimeStr);
  if (!date) return 0;
  const now = new Date();
  return Math.floor((now.getTime() - date.getTime()) / 1000);
}
