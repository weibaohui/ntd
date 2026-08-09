// commandExtractor 单元测试（issue #648）。
//
// 本文件由原 Playwright spec `tests/issue-648-command-extractor.spec.ts` 迁移而来：
// 原写法用 Playwright + Vite dev server 在浏览器里 `import('/src/utils/commandExtractor.ts')`
// 跑「浏览器内模块单测」，依赖独立的 5173 vite 进程（make dev 的 18088 embedded 模式不服务
// /src/* 模块），导致该 spec 长期因连不上 vite 而整批失败。commandExtractor 是纯函数模块
// （无 DOM/React 依赖），完全可在 vitest 里直接 import 断言，无需浏览器与 vite——故迁来，
// 顺带消除「单测依赖 Playwright」的反模式。覆盖范围与原 spec 逐条对应，不增不减。
//
// 覆盖：parseJsonSafe 容错、isBashTool 大小写无关、Claude/Agent/kimi/codex/pi/atomcode/
// codewhale 各协议族提取正确性、extractCommandsByExecutor 分派。

import { describe, it, expect } from 'vitest';
import {
  parseJsonSafe,
  isBashTool,
  extractCommandsByExecutor,
  __test__,
} from './commandExtractor';
import type { LogEntry } from '@/types';

const {
  extractClaudeCommands,
  extractAgentCommands,
  extractKimiCommands,
  extractCodeWhaleCommands,
  extractCodexCommands,
  extractPiCommands,
  extractAtomcodeCommands,
} = __test__;

describe('parseJsonSafe', () => {
  it('合法 JSON 对象原样解析', () => {
    // 基线：能解析才谈得上后续字段读取
    expect(parseJsonSafe('{"a":1}')).toEqual({ a: 1 });
  });

  it('空串 / null / 非法文本 / 数组 一律返回 null（只接受对象）', () => {
    // 容错四连：执行器日志里这些输入常见，必须降级为 null 而非抛错，否则渲染崩溃
    expect(parseJsonSafe('')).toBeNull();
    expect(parseJsonSafe(null)).toBeNull();
    expect(parseJsonSafe('not-json')).toBeNull();
    // 数组虽是合法 JSON，但工具入参约定为对象，数组视为非法（避免误当对象读字段）
    expect(parseJsonSafe('[1,2]')).toBeNull();
  });
});

describe('isBashTool', () => {
  it('大小写无关匹配 Bash/bash/Shell/exec_shell', () => {
    // 不同执行器对「命令工具」命名不一，需统一识别为可提取命令的条目
    expect(isBashTool('Bash')).toBe(true);
    expect(isBashTool('bash')).toBe(true);
    expect(isBashTool('Shell')).toBe(true);
    expect(isBashTool('exec_shell')).toBe(true);
  });

  it('非命令工具 / 空 / undefined 返回 false', () => {
    expect(isBashTool('Read')).toBe(false);
    expect(isBashTool('')).toBe(false);
    expect(isBashTool(undefined)).toBe(false);
  });
});

describe('extractClaudeCommands', () => {
  it('按 toolCallId 配对 command 与 output', () => {
    // Claude 协议靠 toolCallId 把「发起」与「结果」两条日志精确配对（非 FIFO）
    const logs: LogEntry[] = [
      { timestamp: 't1', type: 'tool_use', content: 'x', toolName: 'Bash', toolInputJson: JSON.stringify({ command: 'ls -la' }), toolCallId: 'a1' },
      { timestamp: 't2', type: 'tool_result', content: 'file.txt\ndir/', toolCallId: 'a1', isError: false },
      { timestamp: 't3', type: 'tool_use', content: 'x', toolName: 'Bash', toolInputJson: JSON.stringify({ command: 'git status' }), toolCallId: 'a2' },
      { timestamp: 't4', type: 'tool_result', content: 'fatal: not a git repo', toolCallId: 'a2', isError: true },
    ];
    const result = extractClaudeCommands(logs);
    expect(result).toHaveLength(2);
    expect(result[0].command).toBe('ls -la');
    expect(result[0].output).toBe('file.txt\ndir/');
    expect(result[0].success).toBe(true);
    expect(result[1].command).toBe('git status');
    // isError=true 的结果判定为失败
    expect(result[1].success).toBe(false);
  });
});

describe('extractAgentCommands', () => {
  it('从 state.input.command + state.output 提取', () => {
    // opencode/mimo 等把命令塞进 state.input.command、输出塞进 state.output
    const logs: LogEntry[] = [
      {
        timestamp: 't1', type: 'tool', content: '执行 bash',
        toolName: 'bash',
        toolInputJson: JSON.stringify({
          state: { status: 'success', input: { command: 'ls' }, output: 'a.txt' },
        }),
      },
    ];
    const result = extractAgentCommands(logs);
    expect(result).toHaveLength(1);
    expect(result[0].command).toBe('ls');
    expect(result[0].output).toBe('a.txt');
    expect(result[0].success).toBe(true);
  });
});

describe('extractKimiCommands', () => {
  it('二次解析 stringified arguments 并按 toolCallId 配对', () => {
    // kimi 把 arguments 序列化成字符串再放到 toolInputJson
    const logs: LogEntry[] = [
      {
        timestamp: 't1', type: 'tool_call', content: 'x', toolName: 'Shell',
        toolInputJson: JSON.stringify({ command: 'pwd' }),
      },
      { timestamp: 't2', type: 'tool_result', content: '/home/user', toolCallId: undefined, isError: false },
    ];
    const result = extractKimiCommands(logs);
    expect(result).toHaveLength(1);
    expect(result[0].command).toBe('pwd');
    expect(result[0].output).toBe('/home/user');
  });

  it('toolCallId 成功判定：output 含 error 时 success=false，否则 true', () => {
    // 判定规则：success = !/error/i.test(output||'')，信任 output 文本而非 isError
    const logs: LogEntry[] = [
      // case 1：正常输出 → success=true
      { timestamp: 't1', type: 'tool_call', content: 'x', toolName: 'Shell',
        toolInputJson: JSON.stringify({ command: 'pwd' }), toolCallId: 'k1' },
      { timestamp: 't2', type: 'tool_result', content: '/home/user', toolCallId: 'k1', isError: false },
      // case 2：output 含 Error → success=false
      { timestamp: 't3', type: 'tool_call', content: 'x', toolName: 'Shell',
        toolInputJson: JSON.stringify({ command: 'git status' }), toolCallId: 'k2' },
      { timestamp: 't4', type: 'tool_result', content: 'Error: not a git repo', toolCallId: 'k2', isError: true },
      // case 3：isError=true 但 output 不含 error → 仍判 success=true（信任 output 文本）
      { timestamp: 't5', type: 'tool_call', content: 'x', toolName: 'Shell',
        toolInputJson: JSON.stringify({ command: 'unknown-cmd' }), toolCallId: 'k3' },
      { timestamp: 't6', type: 'tool_result', content: 'command not found', toolCallId: 'k3', isError: true },
    ];
    const result = extractKimiCommands(logs);
    expect(result).toHaveLength(3);
    expect(result[0]).toMatchObject({ command: 'pwd', output: '/home/user', success: true });
    expect(result[1]).toMatchObject({ command: 'git status', output: 'Error: not a git repo', success: false });
    expect(result[2]).toMatchObject({ command: 'unknown-cmd', output: 'command not found', success: true });
  });
});

describe('extractCodeWhaleCommands', () => {
  it('按 exec_shell + status=success 判定结果', () => {
    const logs: LogEntry[] = [
      { timestamp: 't1', type: 'tool_call', content: 'x', toolName: 'exec_shell',
        toolCallId: 'cw1', toolInputJson: JSON.stringify({ command: 'pwd' }) },
      { timestamp: 't2', type: 'tool_result', content: '/home/user', toolCallId: 'cw1',
        toolInputJson: JSON.stringify({ status: 'success' }) },
    ];
    const result = extractCodeWhaleCommands(logs);
    expect(result).toHaveLength(1);
    expect(result[0]).toMatchObject({ command: 'pwd', output: '/home/user', success: true });
  });
});

describe('extractCodexCommands', () => {
  it('支持字符串数组形式的 command（拼接为 && 链）', () => {
    // codex 的 command 可为 string[]，需 join 成单条展示
    const logs: LogEntry[] = [
      { timestamp: 't1', type: 'tool_call', content: 'x', toolName: 'command_execution',
        toolInputJson: JSON.stringify({ command: ['git add .', 'git commit -m test'] }) },
      { timestamp: 't2', type: 'tool_result', content: 'committed', toolCallId: undefined,
        toolInputJson: JSON.stringify({ exit_code: 0, status: 'completed', duration_ms: 123 }) },
    ];
    const result = extractCodexCommands(logs);
    expect(result).toHaveLength(1);
    expect(result[0].command).toBe('git add . && git commit -m test');
    expect(result[0].exitCode).toBe(0);
    expect(result[0].durationMs).toBe(123);
    expect(result[0].success).toBe(true);
  });

  it('FIFO 配对：result 无 toolCallId 时 exit_code/duration_ms 写到真正命中的首条命令', () => {
    // 回归：3 条命令依次 push；result 无 toolCallId，按 FIFO 命中第一条未填 output 的 cmd。
    // 修复前会错误写到 commands[length-1]（最后一条），修复后写到第 1 条。
    const logs: LogEntry[] = [
      { timestamp: 't1', type: 'tool_call', content: 'x', toolName: 'command_execution',
        toolInputJson: JSON.stringify({ command: 'echo first' }) },
      { timestamp: 't2', type: 'tool_call', content: 'x', toolName: 'command_execution',
        toolInputJson: JSON.stringify({ command: 'echo middle' }) },
      { timestamp: 't3', type: 'tool_call', content: 'x', toolName: 'command_execution',
        toolInputJson: JSON.stringify({ command: 'echo last' }) },
      { timestamp: 't4', type: 'tool_result', content: 'first-output', toolCallId: undefined,
        toolInputJson: JSON.stringify({ exit_code: 0, status: 'completed', duration_ms: 50 }) },
    ];
    const result = extractCodexCommands(logs);
    expect(result).toHaveLength(3);
    // FIFO 命中第一条（只看 output 是否为空，与 push 顺序一致）
    expect(result[0]).toMatchObject({ output: 'first-output', exitCode: 0, durationMs: 50, success: true });
    // 中间与最后一条不应被错误填充
    expect(result[1].output).toBeUndefined();
    expect(result[2].output).toBeUndefined();
  });
});

describe('extractPiCommands', () => {
  it('从 toolExecution.args.command 提取', () => {
    const logs: LogEntry[] = [
      { timestamp: 't1', type: 'tool_use', content: 'x', toolName: 'bash',
        toolInputJson: JSON.stringify({ args: { command: 'echo hi' } }) },
      { timestamp: 't2', type: 'tool_result', content: 'x', toolCallId: undefined,
        toolInputJson: JSON.stringify({ output: 'hi', status: 'success' }) },
    ];
    const result = extractPiCommands(logs);
    expect(result).toHaveLength(1);
    expect(result[0]).toMatchObject({ command: 'echo hi', output: 'hi', success: true });
  });

  it('FIFO 配对：call 与 result 均无 toolCallId 时按顺序填到未填的 cmd', () => {
    // 覆盖 applyPiResult 的 fillPiByFifo 分支（PR #656 评审 MEDIUM #2 缺漏）。
    // pushPiCall 不带 toolCallId → 自动生成 id；applyPiResult 的 result 也不带 → 走 FIFO 兜底。
    const logs: LogEntry[] = [
      { timestamp: 't1', type: 'tool_use', content: 'x', toolName: 'bash',
        toolInputJson: JSON.stringify({ args: { command: 'echo first' } }) },
      { timestamp: 't2', type: 'tool_use', content: 'x', toolName: 'bash',
        toolInputJson: JSON.stringify({ args: { command: 'echo second' } }) },
      { timestamp: 't3', type: 'tool_result', content: 'x', toolCallId: undefined,
        toolInputJson: JSON.stringify({ output: 'first-output', status: 'success', duration_ms: 77 }) },
    ];
    const result = extractPiCommands(logs);
    expect(result).toHaveLength(2);
    // FIFO 命中第一条
    expect(result[0]).toMatchObject({ command: 'echo first', output: 'first-output', success: true });
    // durationMs 只在 fillPiByToolCallId 路径设置；FIFO 走 pairByOrder 不携带 duration_ms
    expect(result[0].durationMs).toBeUndefined();
    // 第二条不应被错误填充
    expect(result[1].output).toBeUndefined();
    expect(result[1].success).toBe(false);
  });
});

describe('extractAtomcodeCommands', () => {
  it('从 stderr 风格 content 解析 [tool→ / [tool←', () => {
    const logs: LogEntry[] = [
      { timestamp: 't1', type: 'stderr', content: '[tool→ bash args={"command":"ls -la"}]' },
      { timestamp: 't2', type: 'stderr', content: '[tool← bash OK 39ms]\nfile.txt' },
    ];
    const result = extractAtomcodeCommands(logs);
    expect(result).toHaveLength(1);
    expect(result[0].command).toBe('ls -la');
    expect(result[0].success).toBe(true);
    expect(result[0].durationMs).toBe(39);
    // stderr 行 `[tool← ...]` 前缀之后的剩余内容（命令实际输出）不应被吞掉
    expect(result[0].output).toBe('file.txt');
  });
});

describe('extractCommandsByExecutor', () => {
  it('按执行器名分派到对应协议族', () => {
    // 同一条 Agent 协议日志，不同执行器名应正确路由；未知执行器走 Claude fallback
    const logs: LogEntry[] = [
      { timestamp: 't1', type: 'tool', content: 'x', toolName: 'bash',
        toolInputJson: JSON.stringify({ state: { status: 'success', input: { command: 'ls' }, output: 'a' } }) },
    ];
    expect(extractCommandsByExecutor(logs, 'opencode').length).toBe(1);
    expect(extractCommandsByExecutor(logs, 'mobilecoder').length).toBe(1);
    expect(extractCommandsByExecutor(logs, 'mimo').length).toBe(1);
    // issue #673：zhanlu 与 opencode 输出格式一致，复用 Agent 协议提取
    expect(extractCommandsByExecutor(logs, 'zhanlu').length).toBe(1);
    // hermes 走「不支持」分支，返回 []
    expect(extractCommandsByExecutor(logs, 'hermes').length).toBe(0);
    // 未知执行器走 Claude fallback；上面那条日志不是 Claude 协议，应是 0
    expect(extractCommandsByExecutor(logs, 'something-new').length).toBe(0);
  });
});
