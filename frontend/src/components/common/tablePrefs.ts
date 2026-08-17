/**
 * tablePrefs — 列表表格用户偏好（列宽 + 排序）的 localStorage 读写纯函数层。
 *
 * 设计要点：
 * 1. 纯函数无 React 依赖，可在单测中直接运行。
 * 2. JSON.parse 失败静默返回 null（不信任 localStorage 中的外部输入）。
 * 3. key 白名单校验：只允许 "todos" | "tasks" | "loops" 三个已知 tableKey。
 * 4. 排序 order 白名单校验：只允许 "ascend" | "descend" | null。
 */

/** 排序方向白名单（与 antd SortOrder 对齐）。 */
type SortOrder = 'ascend' | 'descend' | null;

/** 排序状态：field 为列 dataIndex，order 为方向。 */
export interface SortState {
  field: string;
  order: SortOrder;
}

/** 单张表的完整偏好：列宽 map + 当前排序。 */
export interface TablePrefs {
  widths: Record<string, number>;
  sort: SortState;
}

/** localStorage key 前缀，避免与项目其他 key 冲突。 */
const PREFIX = 'ntd_table_prefs:';

/** 允许的 tableKey 白名单：防止未知 key 写入/读取任意 localStorage 数据。 */
const VALID_KEYS = new Set(['todos', 'tasks', 'loops']);

/**
 * 从 localStorage 读取指定表的偏好。
 *
 * 安全策略：
 * - key 不在白名单 → 返回 null（拒绝读取）
 * - JSON.parse 抛异常 → 返回 null（静默容错）
 * - sort.field / sort.order 不在白名单 → 回退到 { field: 'id', order: 'descend' }
 * - widths 中非数字值 → 过滤剔除
 */
export function getTablePrefs(tableKey: string): TablePrefs | null {
  if (!VALID_KEYS.has(tableKey)) return null;
  const raw = localStorage.getItem(PREFIX + tableKey);
  if (raw === null) return null;

  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch {
    return null; // JSON 损坏：静默 fallback，不抛异常
  }

  if (typeof parsed !== 'object' || parsed === null) return null;
  const obj = parsed as Record<string, unknown>;

  // widths：只保留值为有限数字的条目，防止恶意注入
  const widths: Record<string, number> = {};
  if (typeof obj.widths === 'object' && obj.widths !== null) {
    for (const [k, v] of Object.entries(obj.widths as Record<string, unknown>)) {
      if (typeof v === 'number' && Number.isFinite(v) && v > 0) {
        widths[k] = v;
      }
    }
  }

  // sort：field 必须是字符串，order 必须在白名单内，否则回退默认
  const rawSort = obj.sort as Record<string, unknown> | undefined;
  const sort: SortState = {
    field: typeof rawSort?.field === 'string' ? rawSort.field : 'id',
    order:
      rawSort?.order === 'ascend' || rawSort?.order === 'descend'
        ? rawSort.order
        : null,
  };

  return { widths, sort };
}

/**
 * 把偏好写入 localStorage。
 * key 不在白名单时静默拒绝，不抛异常（写失败不影响渲染）。
 */
export function setTablePrefs(tableKey: string, prefs: TablePrefs): void {
  if (!VALID_KEYS.has(tableKey)) return;
  try {
    localStorage.setItem(PREFIX + tableKey, JSON.stringify(prefs));
  } catch {
    // localStorage 写满 / 隐私模式：静默失败，不影响功能
  }
}

/** 清除指定表的偏好，下次访问回退到默认值。 */
export function resetTablePrefs(tableKey: string): void {
  if (!VALID_KEYS.has(tableKey)) return;
  localStorage.removeItem(PREFIX + tableKey);
}
