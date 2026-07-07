// 时间格式化工具。
//
// 历史背景：之前 formatRelativeTime 用 date-fns 的 formatDistanceToNow，
// 与项目里已有 dayjs 形成 ~50KB 的重复依赖。本文件把相对时间部分迁到 dayjs，
// 整体只保留 dayjs 一个日期库。
//
// 为什么 formatLocalDateTime 继续用原生 Date.toLocaleString：
// 它的输出依赖宿主环境本地化设置（如「2026/7/7 下午4:18:33」），
// 迁到 dayjs 后格式会变成固定的 'YYYY-MM-DD HH:mm:ss'，是用户可见的行为变化。
// 现有页面、截图、用户习惯都已固化在 toLocaleString 输出上，保留更稳。

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
 * 后端返回的 ISO 字符串带 Z（UTC），Date 构造器自动识别为 UTC 时间，
 * toLocaleString() 再按本地时区与本地化模板渲染，无需 dayjs 介入。
 */
export function formatLocalDateTime(timeStr: string | null | undefined): string {
  if (!timeStr) return '';
  return new Date(timeStr).toLocaleString();
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
