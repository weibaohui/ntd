// processYamlValidator.ts
// ---------------------------------------------------------------------------
// M3 里程碑：YAML 解析与错误提取的纯函数模块。
//
// 设计意图：
// - 把 js-yaml 的解析逻辑抽成纯函数，便于单元测试（vitest）。
// - Monaco 编辑器组件只消费 parseYaml 的结果，不直接耦合 js-yaml。
// - 错误行号统一转 1-based，因为 js-yaml 的 mark.line 是 0-based，
//   而 Monaco 的 Range 是 1-based。这一步转换集中在这里，避免散落。
//
// M5 里程碑扩展：新增 yamlDump，把 ProcessDefinition 序列化为 YAML 文本。
// 与 parseYaml 配套，支撑 YAML ↔ 可视化双向联动。
// ---------------------------------------------------------------------------

import yaml from 'js-yaml';

// YAML 解析失败时的错误信息，供 Monaco 行号槽标红用。
export interface YamlError {
  // 1-based 行号（js-yaml 的 mark.line 是 0-based，这里 +1）
  line: number;
  // 错误消息（js-yaml 原始 message，通常已含行号信息）
  message: string;
}

// parseYaml 的返回结构，成功时 parsed 为对象、error 为 null；
// 失败时 parsed 为 null、error 含行号与消息。
export interface YamlParseResult {
  // 解析成功时为解析后的对象，失败时为 null
  parsed: unknown | null;
  // 解析失败时的错误信息，成功时为 null
  error: YamlError | null;
}

// 解析 YAML 文本，返回成功对象或错误信息。
//
// 边界处理：
// - 空字符串或纯空白 → { parsed: null, error: null }，不报错（视为空工艺）
// - YAMLException 带 mark.line → 转 1-based
// - YAMLException 无 mark（极少见）→ line 回退为 1，避免 Monaco Range(0,...) 越界
export function parseYaml(yamlText: string): YamlParseResult {
  // 空串或纯空白不视为错误，直接返回空解析结果；
  // 这样 Monaco 在清空内容时不会残留标红。
  if (yamlText.trim() === '') {
    return { parsed: null, error: null };
  }

  try {
    // 用 DEFAULT_SCHEMA（JS-YAML 默认的 JSON 兼容 schema）。
    // 工艺 YAML 只用基本类型（string/int/bool/list/map），DEFAULT_SCHEMA 足够。
    // 不用 DEFAULT_FULL_SCHEMA：它允许 !!js/func 等不安全类型，且 @types/js-yaml 4.x 已移除该常量。
    const result = yaml.load(yamlText, { schema: yaml.DEFAULT_SCHEMA });
    return { parsed: result, error: null };
  } catch (err) {
    // js-yaml 抛出的 YAMLException 自带 mark.line（0-based）和 message。
    // 我们把 0-based 转 1-based 供 Monaco 使用。
    if (err instanceof yaml.YAMLException && err.mark) {
      const line = err.mark.line + 1; // 0-based → 1-based
      return {
        parsed: null,
        error: { line, message: err.message },
      };
    }

    // 极少见：YAMLException 无 mark，或其他类型异常。
    // 行号回退为 1，避免 Monaco Range(0,...) 越界报错。
    const message = err instanceof Error ? err.message : String(err);
    return {
      parsed: null,
      error: { line: 1, message },
    };
  }
}

// ── M5 新增：YAML 序列化 ─────────────────────────────

// 把 ProcessDefinition 对象序列化为 YAML 文本。
//
// 用于"可视化 → YAML"路径：可视化操作更新 definition 后，
// 调用 yamlDump 刷新 Monaco 编辑器内容。
//
// 配置：
// - schema: DEFAULT_SCHEMA（与 parseYaml 一致，只用基本类型）
// - indent: 2（2 空格缩进，与原 bundled YAML 风格一致）
// - lineWidth: -1（禁用行宽折行，避免长文本被拆行）
// - noRefs: true（不输出锚点引用，避免结构复杂化）
//
// 边界处理：
// - null 或 undefined → 空串（与 parseYaml 的空串边界对称）
export function yamlDump(obj: unknown): string {
  if (obj === null || obj === undefined) return '';
  // yaml.dump 已是纯函数，这里薄封装统一 schema/风格
  return yaml.dump(obj, {
    schema: yaml.DEFAULT_SCHEMA,
    indent: 2,
    lineWidth: -1,
    noRefs: true,
  });
}
