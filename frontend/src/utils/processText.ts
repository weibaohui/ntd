// 工艺列统一文本：事项 / 任务 / 环路三个列表页共用。
// 输出格式固定为 `#<工艺id>-<工艺名称>-<工艺版本>`，无工艺来源时返回 '-'。

/**
 * 生成工艺列文本。
 *
 * 回退策略：
 * - id 缺失 / 为 0 / 为负数：整个工艺来源不存在，返回 '-'；
 *   （NTD-013：扩展 id<=0 判空，兼容老委派任务 template_id=Some(0) 的哨兵值，
 *   否则会错误渲染为 #0-#0--。）
 * - name 缺失：用 `#<id>` 兜底，避免出现空名称段；
 * - version 缺失：用 '—' 占位，保持三段式结构可读。
 */
export function formatProcessText(
  id?: number | null,
  name?: string | null,
  version?: string | null,
): string {
  if (id == null || id <= 0) return '-';
  const displayName = name?.trim() ? name : `#${id}`;
  const displayVersion = version?.trim() ? version : '—';
  return `#${id}-${displayName}-${displayVersion}`;
}
