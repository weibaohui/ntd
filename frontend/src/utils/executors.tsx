// 执行器运行时常量与辅助函数。
//
// 从 types/execution.tsx 迁出：类型定义文件不应包含 JSX/运行时代码。
// 保持通过 types/index.tsx re-export，现有 import { EXECUTORS } from '@/types' 无需改动。

import { FaSquare } from 'react-icons/fa';
import type { ExecutorOption, ExecutionRecord } from '@/types/execution';

// ─── Executor 摘要映射 ──────────────────────────────────────

export const EXECUTORS: ExecutorOption[] = [
  { value: 'claudecode', label: 'Claude',    color: '#e17055', icon: <FaSquare color="#e17055" size={14} />, resumable: true },
  { value: 'codebuddy',  label: 'CodeBuddy', color: '#00b894', icon: <FaSquare color="#00b894" size={14} /> },
  { value: 'opencode',   label: 'Opencode',  color: '#fdcb6e', icon: <FaSquare color="#fdcb6e" size={14} />, resumable: true },
  { value: 'mobilecoder', label: 'MobileCoder', color: '#6c5ce7', icon: <FaSquare color="#6c5ce7" size={14} />, resumable: true },
  { value: 'atomcode',   label: 'AtomCode',  color: '#e84393', icon: <FaSquare color="#e84393" size={14} /> },
  { value: 'hermes',     label: 'Hermes',    color: '#0984e3', icon: <FaSquare color="#0984e3" size={14} />, resumable: true },
  { value: 'kimi',       label: 'Kimi',      color: '#d63031', icon: <FaSquare color="#d63031" size={14} />, resumable: true },
  { value: 'codex',      label: 'Codex',     color: '#488597', icon: <FaSquare color="#488597" size={14} /> },
  { value: 'codewhale',  label: 'CodeWhale', color: '#00cec9', icon: <FaSquare color="#00cec9" size={14} />, resumable: true },
  { value: 'pi',        label: 'Pi',        color: '#8e44ad', icon: <FaSquare color="#8e44ad" size={14} />, resumable: true },
  { value: 'mimo',      label: 'MiMo',      color: '#ff6b6b', icon: <FaSquare color="#ff6b6b" size={14} />, resumable: true },
  // Issue #673: 新增 Zhanlu 执行器，与 Opencode 输出格式一致
  // 颜色与下方 EXECUTOR_COLORS.zhanlu 同步为 #0f766e，与 agents(#2d3436) 视觉可分。
  { value: 'zhanlu',    label: 'Zhanlu',    color: '#0f766e', icon: <FaSquare color="#0f766e" size={14} />, resumable: true },
  { value: 'kilo',      label: 'Kilo',      color: '#e67700', icon: <FaSquare color="#e67700" size={14} />, resumable: true },
  // `agents` is read-only skill source (`~/.agents/skills`), not shown in executor management.
  // Included here so it appears in Skills overview/sync tabs.
  { value: 'agents',     label: 'Agents',    color: '#2d3436', icon: <FaSquare color="#2d3436" size={14} /> },
];

export const EXECUTOR_COLORS: Record<string, string> = {
  claudecode: '#e17055',
  codebuddy: '#00b894',
  opencode: '#fdcb6e',
  mobilecoder: '#6c5ce7',
  atomcode: '#e84393',
  hermes: '#0984e3',
  kimi: '#d63031',
  codex: '#488597',
  codewhale: '#00cec9',
  pi: '#8e44ad',
  mimo: '#ff6b6b',
  // Issue #673 + PR #677 review H1：zhanlu 颜色与 agents 撞色（都是 #2d3436），
  // 改为深青 `#0f766e` 与 opencode 的 `#fdcb6e` / agents 的 `#2d3436` 视觉可分。
  zhanlu: '#0f766e',
  kilo: '#e67700',
  agents: '#2d3436',
  // Aliases for backward compatibility with database names
  'claude_code': '#e17055',
  'claude': '#e17055',
  'cbc': '#00b894',
  'atom': '#e84393',
};

export function getExecutorColor(name: string | undefined | null): string {
  if (!name) return '#999';
  return EXECUTOR_COLORS[name] || '#999';
}

export function getExecutorOption(value: string): ExecutorOption {
  return EXECUTORS.find(e => e.value === value.toLowerCase()) || EXECUTORS[0];
}

/** 不包含 `agents` 的执行器列表，用于执行器选择 UI（agents 是只读 skill 来源，不是执行器）。 */
export const EXECUTORS_FOR_PICKER = EXECUTORS.filter(e => e.value !== 'agents');

/** 支持继续对话的执行器 value 集合。从 EXECUTORS 的 resumable 标志自动派生，无需手动维护。 */
export const RESUMABLE_EXECUTORS = new Set(EXECUTORS.filter(e => e.resumable).map(e => e.value));

/// 默认执行器
export const DEFAULT_EXECUTOR = 'claudecode';

/** 仅支持继续对话的执行器（用于选择 UI 的下拉数据源） */
export const RESUMABLE_EXECUTOR_OPTIONS = EXECUTORS.filter(e => RESUMABLE_EXECUTORS.has(e.value));

export function supportsResume(record: ExecutionRecord): boolean {
  return (
    record.status !== 'running' &&
    !!record.session_id &&
    !!record.executor &&
    RESUMABLE_EXECUTORS.has(record.executor.toLowerCase())
  );
}

export function executorConfigToOption(ec: { name: string; display_name: string }): ExecutorOption {
  const color = getExecutorColor(ec.name);
  return {
    value: ec.name,
    label: ec.display_name || ec.name,
    color,
    icon: <FaSquare color={color} size={14} />,
  };
}
