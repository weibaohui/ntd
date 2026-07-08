import { test, expect } from '@playwright/test';
import { parseProposals } from '../src/components/blackboard-proposal/parseProposals';

/**
 * parseProposals 纯函数测试。
 *
 * 这是「黑板 → Todo 建议」闭环里唯一有复杂逻辑的纯函数，必须覆盖：
 * 正常解析 / 空输入 / 非法 YAML / 缺字段 / 代码块包裹 / 带解释文字 / 多条建议。
 *
 * UI 渲染与端到端流程依赖真实 AI 执行（慢且不确定），不纳入自动化，
 * 改由 playwright-cli 在 make dev 环境下手动验证，证据发 PR 评论。
 */
test.describe('parseProposals 黑板建议解析', () => {
  test('正常 YAML 列表应解析出 title 与 prompt', () => {
    const input = '- title: 修复登录超时\n  prompt: 排查登录接口偶发超时的根因并修复';
    const { proposals, raw } = parseProposals(input);
    expect(proposals).toHaveLength(1);
    expect(proposals[0].title).toBe('修复登录超时');
    expect(proposals[0].prompt).toBe('排查登录接口偶发超时的根因并修复');
    expect(raw).toBe(input);
  });

  test('空输入应返回空建议且 raw 为空', () => {
    const { proposals, raw } = parseProposals('');
    expect(proposals).toHaveLength(0);
    expect(raw).toBe('');
  });

  test('非法 YAML 应返回空建议并透出原文（不静默失败）', () => {
    const input = '这不是yaml: [unclosed';
    const { proposals, raw } = parseProposals(input);
    expect(proposals).toHaveLength(0);
    expect(raw).toBe(input);
  });

  test('缺 prompt 字段的项应被丢弃', () => {
    const { proposals } = parseProposals('- title: 只有标题');
    expect(proposals).toHaveLength(0);
  });

  test('title 为空字符串的项应被丢弃', () => {
    const { proposals } = parseProposals('- title: ""\n  prompt: 有 prompt 但无标题');
    expect(proposals).toHaveLength(0);
  });

  test('markdown 代码块包裹的 YAML 应能解析', () => {
    const input = '```yaml\n- title: 任务A\n  prompt: 执行A\n```';
    const { proposals } = parseProposals(input);
    expect(proposals).toHaveLength(1);
    expect(proposals[0].title).toBe('任务A');
    expect(proposals[0].prompt).toBe('执行A');
  });

  test('前后带解释文字的 YAML 应截取列表段解析', () => {
    const input = '好的，以下是建议：\n- title: 任务A\n  prompt: 执行A\n希望对你有帮助';
    const { proposals } = parseProposals(input);
    expect(proposals).toHaveLength(1);
    expect(proposals[0].title).toBe('任务A');
  });

  test('多条建议应全部解析并保留顺序', () => {
    const input = '- title: A\n  prompt: PA\n- title: B\n  prompt: PB';
    const { proposals } = parseProposals(input);
    expect(proposals).toHaveLength(2);
    expect(proposals.map(p => p.title)).toEqual(['A', 'B']);
  });

  // 与 PROPOSAL_PROMPT 约定的「prompt 必须用字面量块标量（|）书写」配套：
  // 验证多行 prompt 经块标量输出后，parseProposals 能完整保留全部续行、不被截断。
  test('block scalar 多行 prompt 应完整保留不截断', () => {
    const input =
      '- title: 修复登录超时\n' +
      '  prompt: |\n' +
      '    排查登录接口偶发超时的根因。\n' +
      '    重点关注数据库连接池与第三方鉴权延迟。\n' +
      '    给出修复方案并落地。';
    const { proposals } = parseProposals(input);
    expect(proposals).toHaveLength(1);
    expect(proposals[0].title).toBe('修复登录超时');
    // 块标量原样保留全部三行，仅由 toProposal 的 trim 去掉块标量尾部换行
    expect(proposals[0].prompt).toBe(
      '排查登录接口偶发超时的根因。\n' +
        '重点关注数据库连接池与第三方鉴权延迟。\n' +
        '给出修复方案并落地。'
    );
  });
});
