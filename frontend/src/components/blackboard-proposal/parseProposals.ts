import yaml from 'js-yaml';

/** 一条 Todo 建议：可读标题 + 可执行 prompt。 */
export interface Proposal {
  title: string;
  prompt: string;
}

/** parseProposals 的返回：解析出的建议列表 + AI 原始输出（兜底透出用）。 */
export interface ParseResult {
  proposals: Proposal[];
  raw: string;
}

/**
 * 从 AI 输出文本中解析 Todo 建议列表。
 *
 * 容错三步（已确认决策：预处理 + 兜底）：
 * 1. 预处理：剥掉 markdown 代码块围栏、截取首个列表项开头的 YAML 数组段，
 *    救回「AI 加了解释文字 / 包了代码块」这两种最常见的「非法 YAML」场景；
 * 2. 解析：js-yaml.load + 逐项校验（必须是对象、含非空 title 与 prompt）；
 * 3. 兜底：任一步失败都返回空 proposals，并把 AI 原文放进 raw 透出给 Drawer，
 *    让用户看见 AI 实际输出，绝不静默吞掉失败。
 */
export function parseProposals(input: string): ParseResult {
  const raw = input ?? '';
  // 步骤 1：预处理，尽量提取「干净的 YAML 列表片段」
  const candidate = extractYamlCandidate(raw);
  // 步骤 2 + 3：解析校验，失败自然回落为空数组（raw 已带原文供兜底）
  const proposals = tryParseProposals(candidate);
  return { proposals, raw };
}

/**
 * 从 AI 原文中提取最可能是「YAML 列表」的片段。
 *
 * 依次尝试：
 * - 若被 \`\`\`yaml ... \`\`\`（或裸 \`\`\`）围栏包裹，取围栏内内容；
 * - 否则从首个「- 」开头的列表项行截取到原文末尾，跳过前后多余的解释文字；
 * - 都不命中则原样返回，交由后续解析失败兜底。
 */
function extractYamlCandidate(raw: string): string {
  const trimmed = raw.trim();
  // 情况 A：markdown 代码块围栏包裹（```yaml / ```yml / 裸 ```）
  const fenceMatch = trimmed.match(/```(?:ya?ml)?\s*\n([\s\S]*?)\n?```/i);
  if (fenceMatch && fenceMatch[1]) {
    return fenceMatch[1].trim();
  }
  // 情况 B：混杂解释文字，截取「首个列表项」开始的连续 YAML 列表段；
  // 遇到非缩进、非 `- ` 开头的行（通常是尾部解释文字）即停止，避免污染解析
  const lines = trimmed.split('\n');
  const firstListItemIndex = lines.findIndex(line => /^\s*-\s+\S/.test(line));
  if (firstListItemIndex >= 0) {
    return collectYamlListLines(lines, firstListItemIndex);
  }
  return trimmed;
}

/**
 * 从起始索引向下收集连续的 YAML 列表行。
 *
 * 保留：`- ` 开头的列表项行、缩进的续行（项内字段）、空行（项间分隔）；
 * 一旦遇到「非缩进且非 `- `」的行，判定为尾部解释文字并截断。
 */
function collectYamlListLines(lines: string[], startIndex: number): string {
  const collected: string[] = [];
  for (let i = startIndex; i < lines.length; i++) {
    const line = lines[i];
    if (/^\s*-\s+\S/.test(line) || /^\s+\S/.test(line) || line.trim() === '') {
      collected.push(line);
    } else {
      break;
    }
  }
  return collected.join('\n').trim();
}

/**
 * 用 js-yaml 解析候选文本为 Proposal[]。
 *
 * 严格校验：顶层必须是数组；每个元素必须是对象且含非空字符串 title 与 prompt。
 * 任何解析异常或校验不过都返回空数组（调用方用 raw 兜底）。
 */
function tryParseProposals(candidate: string): Proposal[] {
  let loaded: unknown;
  try {
    loaded = yaml.load(candidate);
  } catch {
    return [];
  }
  if (!Array.isArray(loaded)) {
    return [];
  }
  return loaded.map(toProposal).filter((p): p is Proposal => p !== null);
}

/** 把单个 YAML 元素规整为 Proposal；字段缺失或类型不符返回 null（被过滤掉）。 */
function toProposal(item: unknown): Proposal | null {
  if (typeof item !== 'object' || item === null) {
    return null;
  }
  const obj = item as Record<string, unknown>;
  const title = typeof obj.title === 'string' ? obj.title.trim() : '';
  const prompt = typeof obj.prompt === 'string' ? obj.prompt.trim() : '';
  // title / prompt 任一为空都视为无效建议，丢弃
  if (!title || !prompt) {
    return null;
  }
  return { title, prompt };
}
