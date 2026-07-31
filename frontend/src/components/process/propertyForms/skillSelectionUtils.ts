// skillSelectionUtils.ts
// ---------------------------------------------------------------------------
// 工艺环节 Skills 选择器的纯逻辑工具（需求 053）。
//
// 背景：环节的 link.skills 是一个字符串数组，来源有两类——
//   1) 从该执行器 Skills 列表里勾选的（名字精确等于列表项 name）；
//   2) 用户手填的"自定义"技能名（可能不在当前列表里）。
// 下方的多选 Table 只承载第 1 类（rowKey=name），第 2 类只活在已选 Tag 区。
// 因此需要一组桥接函数，在「表格勾选态」与「完整已选数组」之间互转，
// 保证手填项不会因表格勾选变化而被误删。全部为纯函数，便于单测。
// ---------------------------------------------------------------------------
import type { SkillMeta } from '@/types';

/** 已选技能名数组（可能为 undefined）的归一化类型别名。 */
type Selected = readonly string[] | undefined;

/**
 * 把已选数组拆成「列表内选中」与「手填自定义」两组。
 * - inList：名字精确命中 listNames，会出现在表格里被勾选；
 * - custom：不在 listNames，仅活在已选 Tag 区。
 * 用精确相等判断（表格 rowKey=name 是精确匹配）；大小写不同的变体归入 custom。
 */
export function splitSelected(
  selected: Selected,
  listNames: ReadonlySet<string>,
): { inList: string[]; custom: string[] } {
  const inList: string[] = [];
  const custom: string[] = [];
  for (const name of selected ?? []) {
    // 精确命中列表才算「列表内」，避免把大小写变体误当列表项
    if (listNames.has(name)) {
      inList.push(name);
    } else {
      custom.push(name);
    }
  }
  return { inList, custom };
}

/**
 * 表格勾选变化时，重新拼出完整已选数组：
 * 保留原有 custom（手填项，表格管不到）+ 当前勾选的列表内项。
 * 顺序：custom 保持原 selected 相对顺序在前，checkedKeys 在后，让 Tag 区顺序稳定。
 */
export function syncFromTable(
  selected: Selected,
  listNames: ReadonlySet<string>,
  checkedKeys: readonly string[],
): string[] {
  // 只需取出 custom 与新勾选合并；inList 被勾选态整体替换
  const { custom } = splitSelected(selected, listNames);
  // 去重：某执行器的 skills 数组可能含重复 skill 名（如 claudecode 有两个 code-refactoring），
  // 勾选时 checkedKeys 可能带重复；已选数组不允许同名重复，否则已选 Tag 区会重复显示。
  return [...new Set([...custom, ...checkedKeys])];
}

/**
 * 判断一个搜索词能否作为「自定义技能」手填加入：
 * 非空、且不在当前列表内（大小写不敏感，避免与列表项重复）、
 * 且未在已选中（大小写不敏感，避免重复添加）。
 * 用大小写不敏感防止 Deploy/deploy 造成重复。
 */
export function canAddCustom(
  selected: Selected,
  listNames: ReadonlySet<string>,
  keyword: string,
): boolean {
  const norm = keyword.trim().toLowerCase();
  // 空输入直接否决，避免误加空技能名
  if (!norm) return false;
  // 列表里已有（忽略大小写）→ 不应手填，引导用户去表格勾选
  if (hasNameIgnoreCase(listNames, norm)) return false;
  // 已选里已有（忽略大小写）→ 重复，不加
  const selectedLower = new Set((selected ?? []).map((s) => s.toLowerCase()));
  return !hasNameIgnoreCase(selectedLower, norm);
}

/** 名字集合中是否存在与 norm（已小写）相等的项（大小写不敏感包含判断）。 */
function hasNameIgnoreCase(names: ReadonlySet<string>, norm: string): boolean {
  for (const n of names) {
    if (n.toLowerCase() === norm) return true;
  }
  return false;
}

/**
 * 手填加入一个自定义技能名：trim 后去重（大小写不敏感）再追加。
 * 空值或已存在（含大小写变体）则原样返回，不做变更。
 * 保留用户输入的原大小写（运行时按名字匹配执行器 skill）。
 */
export function addCustom(selected: Selected, name: string): string[] {
  const trimmed = name.trim();
  const list = [...(selected ?? [])];
  // 空字符串无意义，直接返回副本，保持引用稳定
  if (!trimmed) return list;
  const norm = trimmed.toLowerCase();
  if (list.some((s) => s.toLowerCase() === norm)) return list;
  return [...list, trimmed];
}

/** 从已选中移除指定名字（精确相等）。不存在则返回原数组的副本。 */
export function removeSkill(selected: Selected, name: string): string[] {
  return (selected ?? []).filter((s) => s !== name);
}

/**
 * 按关键字过滤技能列表：匹配 name / description / keywords，大小写不敏感。
 * 空关键字返回全部。逻辑搬自原 SkillSelector 的过滤，保持一致体验。
 */
export function filterSkills(
  skills: readonly SkillMeta[],
  keyword: string,
): SkillMeta[] {
  const norm = keyword.trim().toLowerCase();
  if (!norm) return [...skills];
  return skills.filter(
    (s) =>
      s.name.toLowerCase().includes(norm) ||
      (s.description?.toLowerCase().includes(norm) ?? false) ||
      s.keywords.some((k) => k.toLowerCase().includes(norm)),
  );
}

/**
 * 已选 skill 的 Tag 元数据（颜色 + 标注）。
 * 设计：skill 与执行器解耦，来源标注随筛选切换时有时无（不稳定）且语义弱，
 * 因此统一为只区分「手填自定义」（全量执行器都不存在的 skill，橙）与「已知 skill」（蓝无标注），
 * 标注稳定、与筛选执行器无关。
 */
export function skillTagMeta(
  name: string,
  skillSource: ReadonlyMap<string, string[]>,
): { color: 'blue' | 'orange'; suffix: string } {
  const known = (skillSource.get(name) ?? []).length > 0;
  return known
    ? { color: 'blue', suffix: '' }
    : { color: 'orange', suffix: ' ·自定义' };
}
