// processYamlValidator.test.ts
// ---------------------------------------------------------------------------
// M3 里程碑：processYamlValidator 的 vitest 单元测试。
//
// 覆盖场景：
// 1. 合法 YAML → 返回解析对象，无错误
// 2. 空字符串 / 纯空白 → parsed=null, error=null（不报错）
// 3. 语法错误（Tab 缩进）→ 返回错误行号
// 4. 未闭合单引号 → 返回错误行号
// 5. 多余冒号（第 3 行）→ 返回错误行号（1-based=4）
// 6. 未闭合方括号 → 返回错误行号
// 7. 重复键 → 返回错误行号
// 8. 复杂合法工艺 YAML → 正确解析
//
// 行号映射说明（探测 js-yaml ^4.1.1 的真实行为）：
// - err.mark.line 是 0-based，指向错误 token 所在行
// - parseYaml 内部 +1 转 1-based 供 Monaco Range 使用
// - 部分场景 js-yaml 在错误行的下一行才报错（mark.line 指向下一行），
//   这是 js-yaml 的解析器行为，测试预期需与之对齐。
// ---------------------------------------------------------------------------

import { describe, it, expect } from 'vitest';
import { parseYaml } from './processYamlValidator';

describe('parseYaml', () => {
  // 合法 YAML：简单键值对，验证正常解析路径
  it('parseYaml_validYaml_returnsParsedObject', () => {
    const result = parseYaml('name: test\nversion: 1.0.0\n');
    expect(result.error).toBeNull();
    expect(result.parsed).toEqual({ name: 'test', version: '1.0.0' });
  });

  // 空字符串：不视为错误，返回空解析结果
  // 理由：Monaco 清空内容时不应残留标红
  it('parseYaml_emptyString_returnsNullParsedNoError', () => {
    const result = parseYaml('');
    expect(result.parsed).toBeNull();
    expect(result.error).toBeNull();
  });

  // 纯空白：与空字符串同等处理
  it('parseYaml_whitespaceOnly_returnsNullParsedNoError', () => {
    const result = parseYaml('   \n  \n  ');
    expect(result.parsed).toBeNull();
    expect(result.error).toBeNull();
  });

  // Tab 缩进错误：YAML 不允许用 Tab 缩进
  // 探测结果：mark.line=1（0-based），+1 后 1-based=2
  it('parseYaml_tabIndentError_returnsErrorWithLine', () => {
    // 第 2 行用 Tab 缩进，js-yaml 在第 2 行抛错（mark.line=1，0-based）
    const result = parseYaml('parent:\n\tchild: value\n');
    expect(result.parsed).toBeNull();
    expect(result.error).not.toBeNull();
    expect(result.error!.line).toBe(2);
    expect(result.error!.message).toBeTruthy();
  });

  // 未闭合单引号：验证 js-yaml 报错行号
  // 探测结果：mark.line=1（0-based），+1 后 1-based=2
  it('parseYaml_unclosedSingleQuote_returnsErrorWithLine', () => {
    // 第 1 行单引号未闭合，js-yaml 在第 2 行才检测到流结束（mark.line=1，0-based）
    const result = parseYaml("name: 'unclosed\n");
    expect(result.parsed).toBeNull();
    expect(result.error).not.toBeNull();
    expect(result.error!.line).toBe(2);
  });

  // 多余冒号：验证多行 YAML 中错误出现在第 3 行时的行号
  // 探测结果：mark.line=3（0-based），+1 后 1-based=4
  // 说明：js-yaml 在解析到第 4 行开头时才发现第 3 行的映射键有问题
  it('parseYaml_extraColonOnThirdLine_returnsErrorWithLine', () => {
    // 第 3 行 "bad:value:extra" 有多余冒号
    const result = parseYaml('name: test\nversion: 1.0.0\nbad:value:extra\n');
    expect(result.parsed).toBeNull();
    expect(result.error).not.toBeNull();
    expect(result.error!.line).toBe(4);
  });

  // 未闭合方括号：验证 flow collection 未闭合时报错
  // 探测结果：mark.line=1（0-based），+1 后 1-based=2
  it('parseYaml_unclosedBracket_returnsErrorWithLine', () => {
    // 第 1 行方括号未闭合，js-yaml 在第 2 行才检测到流结束
    const result = parseYaml('list: [a, b\n');
    expect(result.parsed).toBeNull();
    expect(result.error).not.toBeNull();
    expect(result.error!.line).toBe(2);
  });

  // 重复键：验证 YAML 重复键报错
  // 探测结果：mark.line=1（0-based），+1 后 1-based=2
  it('parseYaml_duplicateKey_returnsErrorWithLine', () => {
    // 第 2 行重复定义键 a，js-yaml 在第 2 行抛错
    const result = parseYaml('a: 1\na: 2\n');
    expect(result.parsed).toBeNull();
    expect(result.error).not.toBeNull();
    expect(result.error!.line).toBe(2);
  });

  // 复杂合法工艺 YAML：模拟真实工艺定义结构
  // 验证：phases/links 嵌套结构能被正确解析
  it('parseYaml_complexProcessYaml_parsesCorrectly', () => {
    const yamlText = `process:
  name: test-process
  display_name: 测试工艺
phases:
  - id: phase1
    name: 阶段1
    links:
      - id: link1
        name: 环节1
        step_template: default
        on_success: next
`;
    const result = parseYaml(yamlText);
    expect(result.error).toBeNull();
    expect(result.parsed).not.toBeNull();
    const parsed = result.parsed as Record<string, unknown>;
    expect(parsed['phases']).toBeDefined();
  });
});

// ── M5 新增：yamlDump 测试 ─────────────────────────

import { yamlDump } from './processYamlValidator';

describe('yamlDump', () => {
  it('yamlDump_null_returnsEmptyString', () => {
    expect(yamlDump(null)).toBe('');
  });

  it('yamlDump_undefined_returnsEmptyString', () => {
    expect(yamlDump(undefined)).toBe('');
  });

  it('yamlDump_simpleObject_returnsYamlText', () => {
    // 序列化后应包含 key: value 风格
    const result = yamlDump({ name: 'test', count: 3 });
    expect(result).toContain('name: test');
    expect(result).toContain('count: 3');
  });

  it('yamlDump_nestedObject_returnsIndentedYaml', () => {
    // 嵌套对象应缩进
    const result = yamlDump({ process: { name: 'inner' } });
    expect(result).toContain('process:');
    expect(result).toContain('  name: inner');
  });

  it('yamlDump_array_returnsYamlList', () => {
    // 数组应输出 YAML 列表风格（- item）
    const result = yamlDump({ phases: [{ id: 'p1' }, { id: 'p2' }] });
    expect(result).toContain('phases:');
    expect(result).toContain('- id: p1');
    expect(result).toContain('- id: p2');
  });

  it('yamlDump_roundTripWithParseYaml', () => {
    // dump 后再 parse 应还原对象结构
    const original = { process: { name: 'rt', display_name: '往返' }, phases: [{ id: 'p1', name: '阶段1' }] };
    const dumped = yamlDump(original);
    const parsed = parseYaml(dumped);
    expect(parsed.error).toBeNull();
    expect(parsed.parsed).toEqual(original);
  });
});
