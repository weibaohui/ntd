// 执行器运行时常量与辅助函数。
//
// 从 types/execution.tsx 迁出：类型定义文件不应包含 JSX/运行时代码。
// 保持通过 types/index.tsx re-export，现有 import { EXECUTORS } from '@/types' 无需改动。

import { FaSquare } from 'react-icons/fa';
import type { ExecutorOption, ExecutionRecord } from '@/types/execution';
import * as db from '@/utils/database';

// ─── 全局默认执行器缓存 ──────────────────────────────────────
//
// 模块级缓存：应用启动时从后端加载 is_default=true 的执行器，
// 后续通过 getDefaultExecutor() 同步读取，避免每次都发请求。
// 若加载失败或尚未加载，回退到 DEFAULT_EXECUTOR 常量（claudecode）。

let cachedDefaultExecutor: string | null = null;
let defaultExecutorLoading: Promise<void> | null = null;

/** 从后端加载默认执行器并缓存。应用启动时调用一次即可。 */
export async function loadDefaultExecutor(): Promise<void> {
  // 防止重复请求：已在加载中则复用同一个 Promise
  if (defaultExecutorLoading) return defaultExecutorLoading;

  defaultExecutorLoading = (async () => {
    try {
      const result = await db.getDefaultExecutor();
      if (result?.name) {
        cachedDefaultExecutor = result.name;
      }
    } catch (err) {
      // 加载失败时静默使用常量回退，不阻塞应用启动
      console.warn('加载默认执行器失败，使用回退值:', err);
    }
  })();

  return defaultExecutorLoading;
}

/** 获取当前默认执行器名称（同步读取缓存）。
 *  优先返回从后端加载的缓存值，未加载或加载失败时回退到 DEFAULT_EXECUTOR 常量。
 */
export function getDefaultExecutor(): string {
  return cachedDefaultExecutor || DEFAULT_EXECUTOR;
}

/** 设置默认执行器缓存（在前端修改默认执行器后调用，同步更新本地缓存）。 */
export function setDefaultExecutorCache(name: string): void {
  cachedDefaultExecutor = name;
}

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

// 执行器名称别名映射：数据库中可能存储的历史名称 → EXECUTORS 数组中的规范值。
// 用于统一 getExecutorOption 和 supportsResume 的查找逻辑，避免 alias 无法匹配。
const EXECUTOR_ALIASES: Record<string, string> = {
  // 'claude' / 'claude_code' 是早期数据库写入的名称，统一映射到 'claudecode'
  'claude': 'claudecode',
  'claude_code': 'claudecode',
  // 'cbc' 是 CodeBuddy 的旧称，映射到规范值 'codebuddy'
  'cbc': 'codebuddy',
  // 'atom' 是 AtomCode 的旧称，映射到规范值 'atomcode'
  'atom': 'atomcode',
};

// 将执行器名称（可能是 alias）规范化为 EXECUTORS 数组中的 canonical value。
// toLowerCase 后先查 alias 映射，找不到则返回小写本身（已是规范名或未知名）。
function normalizeExecutorName(name: string): string {
  const lower = name.toLowerCase();
  // alias 存在时返回规范名，否则返回 lowercase 本身（兼容规范名和未来新执行器）
  return EXECUTOR_ALIASES[lower] || lower;
}

export function getExecutorColor(name: string | undefined | null): string {
  if (!name) return '#999';
  return EXECUTOR_COLORS[name] || '#999';
}

export function getExecutorOption(value: string): ExecutorOption {
  // 先规范化 alias（如 'cbc' -> 'codebuddy'），再在 EXECUTORS 中查找，
  // 避免 alias 直接查找失败、错误回退到 EXECUTORS[0]（claudecode）。
  const normalized = normalizeExecutorName(value);
  return EXECUTORS.find(e => e.value === normalized) || EXECUTORS[0];
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
    // 先规范化 alias（如 'claude' -> 'claudecode'），再判断是否可恢复，
    // 避免数据库中存储的旧名称（'claude'/'claude_code' 等）被误判为不可恢复。
    RESUMABLE_EXECUTORS.has(normalizeExecutorName(record.executor))
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
