// 时间格式化工具：用 dayjs 统一处理 ISO 字符串解析、本地时区、相对时间。
// 之前依赖 date-fns，现整体迁移到 dayjs 与 Dashboard 组件保持一致，
// 减少一个 ~50KB 的依赖。

import dayjs from 'dayjs';
import relativeTime from 'dayjs/plugin/relativeTime';
// dayjs 的中文 locale 包是按需引入的独立子模块，
// 直接 import 触发 side-effect 注册到 dayjs 内部 locale 表。
import 'dayjs/locale/zh-cn';

// 注册 relativeTime 插件，提供 .fromNow() / .toNow() 等方法。
// 插件只需注册一次，模块级 import 保证在首次调用前完成。
dayjs.extend(relativeTime);
// 全局默认 locale 设为中文，所有 dayjs 实例（包括 fromNow）都受影响。
dayjs.locale('zh-cn');

/**
 * 格式化为本地可读字符串。
 *
 * 后端返回的 ISO 字符串带 Z（UTC），dayjs 解析后按本地时区展示；
 * 用 'YYYY-MM-DD HH:mm:ss' 固定格式，避免 toLocaleString 在不同浏览器
 * 与 Node 环境下输出不一致导致快照/E2E 测试不稳定。
 */
export function formatLocalDateTime(timeStr: string | null | undefined): string {
  if (!timeStr) return '';
  return dayjs(timeStr).format('YYYY-MM-DD HH:mm:ss');
}

/**
 * 格式化为相对时间（多久之前），例如「5 分钟前」「2 小时前」。
 *
 * fromNow 依赖 relativeTime 插件 + zh-cn locale，
 * 文案与原 date-fns formatDistanceToNow 接近，迁移后视觉无显著差异。
 */
export function formatRelativeTime(timeStr: string | null | undefined): string {
  if (!timeStr) return '';
  return dayjs(timeStr).fromNow();
}

/** 格式化时长（秒）为人类可读字符串 */
export { formatDurationSec } from './format';
