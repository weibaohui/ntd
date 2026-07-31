// gateLabel / gateDetailText / DetailSection / DetailField 单元测试。
// 覆盖：门禁类型映射、条件拼接、空值与边界条件。
import { describe, expect, it } from 'vitest';
import { gateLabel, gateDetailText } from './TaskDetailTabs';
import type { GateDefinition } from '@/types/process';

describe('gateLabel', () => {
  // 已知类型映射为中文标签
  it('test_gateLabel_artifact_present_returns_产物存在', () => {
    expect(gateLabel('artifact_present')).toBe('产物存在');
  });

  it('test_gateLabel_ai_criteria_review_returns_AI评审', () => {
    expect(gateLabel('ai_criteria_review')).toBe('AI 评审');
  });

  it('test_gateLabel_human_approval_returns_人工审批', () => {
    expect(gateLabel('human_approval')).toBe('人工审批');
  });

  it('test_gateLabel_script_check_returns_脚本校验', () => {
    expect(gateLabel('script_check')).toBe('脚本校验');
  });

  // 未知类型回退原字符串
  it('test_gateLabel_unknown_type_returns_raw', () => {
    expect(gateLabel('custom_gate_type')).toBe('custom_gate_type');
  });

  // 空字符串也原样回退
  it('test_gateLabel_empty_string_returns_empty', () => {
    expect(gateLabel('')).toBe('');
  });
});

describe('gateDetailText', () => {
  // AI 评审：min_score + timeout_secs 拼接
  it('test_gateDetailText_ai_review_with_score_and_timeout', () => {
    const gate: GateDefinition = {
      name: 'code-review',
      type: 'ai_criteria_review',
      min_score: 80,
      timeout_secs: 120,
    };
    const text = gateDetailText(gate);
    expect(text).toContain('阈值 ≥ 80 分');
    expect(text).toContain('等待 ≤ 120s');
  });

  it('test_gateDetailText_ai_review_score_only', () => {
    const gate: GateDefinition = {
      name: 'review',
      type: 'ai_criteria_review',
      min_score: 60,
    };
    expect(gateDetailText(gate)).toBe('阈值 ≥ 60 分');
  });

  it('test_gateDetailText_ai_review_timeout_only', () => {
    const gate: GateDefinition = {
      name: 'review',
      type: 'ai_criteria_review',
      timeout_secs: 300,
    };
    expect(gateDetailText(gate)).toBe('等待 ≤ 300s');
  });

  // 产物存在：artifact 字段拼接
  it('test_gateDetailText_artifact_present', () => {
    const gate: GateDefinition = {
      name: 'check-artifact',
      type: 'artifact_present',
      artifact: 'report.pdf',
    };
    expect(gateDetailText(gate)).toContain('产物 report.pdf');
  });

  // 脚本校验：script 字段拼接
  it('test_gateDetailText_script_check', () => {
    const gate: GateDefinition = {
      name: 'run-test',
      type: 'script_check',
      script: 'npm test',
    };
    expect(gateDetailText(gate)).toContain('脚本 npm test');
  });

  // 无匹配条件：空字符串
  it('test_gateDetailText_ai_review_no_config', () => {
    const gate: GateDefinition = {
      name: 'empty-review',
      type: 'ai_criteria_review',
    };
    expect(gateDetailText(gate)).toBe('');
  });

  // 未知类型无配置
  it('test_gateDetailText_unknown_type', () => {
    const gate: GateDefinition = {
      name: 'custom',
      type: 'unknown_type',
    };
    expect(gateDetailText(gate)).toBe('');
  });
});
