// buildEmptyProcessYaml.test.ts
// ---------------------------------------------------------------------------
// M6 里程碑：buildEmptyProcessYaml 的 vitest 单元测试。
//
// 覆盖场景（对应 docs/design/029-M6-新建工艺流程-方案.md §5.1 + §3.1.2）：
// 1. 仅必填字段 → 输出含 name + display_name，无可选字段
// 2. 全字段 → 输出含所有元信息
// 3. phases 恒为空数组
// 4. dump 后再 parse 能还原结构（roundTrip）
// ---------------------------------------------------------------------------

import { describe, it, expect } from 'vitest';
import { buildEmptyProcessYaml } from './buildEmptyProcessYaml';
import { parseYaml } from './processYamlValidator';

describe('buildEmptyProcessYaml', () => {
  it('buildEmptyProcessYaml_minimalFields', () => {
    // 仅 name + display_name，输出应含必填字段、无可选字段
    const yaml = buildEmptyProcessYaml({
      name: 'test-process',
      display_name: '测试工艺',
    });
    expect(yaml).toContain('name: test-process');
    expect(yaml).toContain('display_name: 测试工艺');
    // 未提供可选字段不应输出
    expect(yaml).not.toContain('description:');
    expect(yaml).not.toContain('category:');
    expect(yaml).not.toContain('complexity:');
    expect(yaml).not.toContain('version:');
  });

  it('buildEmptyProcessYaml_allFields', () => {
    // 全字段，输出应含所有元信息
    const yaml = buildEmptyProcessYaml({
      name: 'full-process',
      display_name: '完整工艺',
      description: '这是一个测试工艺',
      category: 'research',
      complexity: 'complex',
      version: '2.1.0',
    });
    expect(yaml).toContain('name: full-process');
    expect(yaml).toContain('display_name: 完整工艺');
    expect(yaml).toContain('description: 这是一个测试工艺');
    expect(yaml).toContain('category: research');
    expect(yaml).toContain('complexity: complex');
    expect(yaml).toContain('version: 2.1.0');
  });

  it('buildEmptyProcessYaml_emptyPhases', () => {
    // phases 恒为空数组（空工艺）
    const yaml = buildEmptyProcessYaml({
      name: 'empty',
      display_name: '空工艺',
    });
    // yaml.dump 输出空数组为 `phases: []`（行内风）
    expect(yaml).toContain('phases: []');
  });

  it('buildEmptyProcessYaml_dumpRoundTrip', () => {
    // dump 后再 parse 能还原结构
    const meta = {
      name: 'rt',
      display_name: '往返',
      description: '往返测试',
      category: 'writing',
      complexity: 'standard',
      version: '1.0.0',
    };
    const yaml = buildEmptyProcessYaml(meta);
    const parsed = parseYaml(yaml);
    expect(parsed.error).toBeNull();
    expect(parsed.parsed).not.toBeNull();
    const obj = parsed.parsed as Record<string, unknown>;
    expect(obj['process']).toEqual({
      name: 'rt',
      display_name: '往返',
      description: '往返测试',
      category: 'writing',
      complexity: 'standard',
      version: '1.0.0',
    });
    expect(obj['phases']).toEqual([]);
  });

  it('buildEmptyProcessYaml_emptyStringOptionalSkipped', () => {
    // 空串可选字段应被跳过（与 undefined 一致）
    const yaml = buildEmptyProcessYaml({
      name: 'skip',
      display_name: '跳过空串',
      description: '',
      category: '',
      complexity: '',
      version: '',
    });
    expect(yaml).toContain('name: skip');
    expect(yaml).toContain('display_name: 跳过空串');
    // 空串字段不应输出
    expect(yaml).not.toContain('description:');
    expect(yaml).not.toContain('category:');
    expect(yaml).not.toContain('complexity:');
    expect(yaml).not.toContain('version:');
  });
});
