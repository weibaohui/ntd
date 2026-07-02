/**
 * 从 AI 生成的结果中提取标题。
 *
 * 主要策略：提取 RESULT 标记包裹的内容。
 * 兜底策略：如果 RESULT 标记不存在，回退到通用解析。
 *
 * 期望的 AI 输出格式：
 * RESULT
 * 优化后的标题文本
 * RESULT
 *
 * 示例输入/输出：
 * - "RESULT\n登录超时问题修复\nRESULT" → "登录超时问题修复"
 * - "RESULT\n**登录超时问题修复**\nRESULT" → "登录超时问题修复"
 * - "登录超时问题修复" → "登录超时问题修复"（兜底）
 */
export function extractTitle(result: string): string {
  if (!result) return '';

  // 1. 主策略：提取 RESULT 标记包裹的内容
  const resultMatch = result.match(/RESULT\s*\n([\s\S]*?)\n\s*RESULT/i);
  if (resultMatch) {
    return cleanMarkdown(resultMatch[1].trim());
  }

  // 2. 兜底策略：通用解析
  let text = result.trim();

  // 尝试提取 markdown 标题 (# Title)
  const headingMatch = text.match(/^#{1,6}\s+(.+)$/m);
  if (headingMatch) {
    return cleanMarkdown(headingMatch[1].trim());
  }

  // 尝试提取加粗文本 (**text** 或 __text__)
  const boldMatch = text.match(/\*\*(.+?)\*\*|__(.+?)__/);
  if (boldMatch) {
    return cleanMarkdown((boldMatch[1] || boldMatch[2]).trim());
  }

  // 如果只有一行，直接返回
  const lines = text.split('\n').filter(l => l.trim());
  if (lines.length === 0) {
    return '';
  }
  if (lines.length === 1) {
    return cleanMarkdown(lines[0].trim());
  }

  // 多行文本：返回第一行
  return cleanMarkdown(lines[0].trim());
}

/**
 * 清理 markdown 格式标记。
 */
function cleanMarkdown(text: string): string {
  return text
    .replace(/\*\*(.+?)\*\*/g, '$1')   // **bold**
    .replace(/__(.+?)__/g, '$1')         // __bold__
    .replace(/\*(.+?)\*/g, '$1')         // *italic*
    .replace(/_(.+?)_/g, '$1')           // _italic_
    .replace(/`(.+?)`/g, '$1')           // `code`
    .replace(/~~(.+?)~~/g, '$1')         // ~~strikethrough~~
    .trim();
}
