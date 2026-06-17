/**
 * issue #648: 命令视图提取器
 *
 * 从 `LogEntry[]` 中按执行器协议提取「命令+返回」对（CommandEntry[]）。
 *
 * ## 设计要点
 *
 * - 不同执行器（Claude / Agent / Function-call / Codex / Pi / Atomcode / Hermes）
 *   的日志格式差异很大，但**用户视角下的"命令+返回"是统一的**。本文件把协议差异
 *   收拢在分派函数 `extractCommandsByExecutor` 后面，对外只暴露归一化的
 *   `CommandEntry`。
 *
 * - 关联策略：优先用 `toolCallId`（后端透出时），否则按执行器协议降级为
 *   顺序配对（FIFO），避免改动后端协议即可工作。
 *
 * - 容错：`parseJsonSafe` 永远返回结构体而非抛错；`isBashTool` 用大小写无关
 *   比较；遇到不识别的日志条直接跳过。
 *
 * ## 单测
 *
 * 与本文件同目录的 `commandExtractor.test.ts` 覆盖各执行器分支（Playwright
 * 测试在 `frontend/tests/` 验证 UI 渲染）。
 */
import type { LogEntry, CommandEntry } from '@/types';

/**
 * 安全解析 JSON 字符串。
 *
 * 任何解析失败（空串、非法 JSON、值非对象）都返回 `null`，**不抛错**。
 * 这是因为日志字段来自执行器流式输出，格式漂移是常态。
 */
export function parseJsonSafe(input: string | null | undefined): Record<string, unknown> | null {
  if (!input) return null;
  try {
    const v = JSON.parse(input);
    if (v && typeof v === 'object' && !Array.isArray(v)) {
      return v as Record<string, unknown>;
    }
    return null;
  } catch {
    return null;
  }
}

/**
 * 判断工具名是否为"类 Bash"工具。
 *
 * 统一小写比较，覆盖：
 * - Claude 协议族：Bash
 * - Agent 协议族：bash / shell
 * - Function-call 协议：Shell / Bash / exec_shell
 */
export function isBashTool(name: string | null | undefined): boolean {
  if (!name) return false;
  const lower = name.toLowerCase();
  return ['bash', 'shell', 'exec_shell'].includes(lower);
}

/** 顺序配对：FIFO 找到最早一个没有填 output 的命令 */
function pairByOrder(commands: CommandEntry[], output: string, isError: boolean | undefined, log: LogEntry): boolean {
  for (const cmd of commands) {
    if (cmd.output === undefined) {
      cmd.output = output;
      cmd.success = !isError;
      if (log.timestamp) cmd.timestamp = log.timestamp;
      return true;
    }
  }
  return false;
}

// ─── 各执行器提取器 ──────────────────────────────────────────

/**
 * Group A: Claude 协议族（claudecode / codebuddy）
 *
 * - 调用：log_type=tool_use, toolName="Bash", toolInputJson.command
 * - 返回：log_type=tool_result, content 为结果, is_error
 * - 关联：优先 toolCallId，否则 FIFO
 */
function extractClaudeCommands(logs: LogEntry[]): CommandEntry[] {
  const commands: CommandEntry[] = [];
  for (const log of logs) {
    if ((log.type === 'tool_use' || log.type === 'tool_call') && isBashTool(log.toolName)) {
      const input = parseJsonSafe(log.toolInputJson) || parseJsonSafe(log.content);
      const cmd = (input?.command as string) || '';
      commands.push({
        id: log.toolCallId || `cmd-${commands.length}-${log.timestamp}`,
        toolName: log.toolName || 'Bash',
        command: cmd,
        args: input ? JSON.stringify(input, null, 2) : log.content,
        success: false,
        timestamp: log.timestamp,
      });
    } else if (log.type === 'tool_result') {
      // 优先按 toolCallId 关联
      let matched = false;
      if (log.toolCallId) {
        const cmd = commands.find(c => c.id === log.toolCallId);
        if (cmd) {
          cmd.output = log.toolResult ?? log.content;
          cmd.success = !log.isError;
          matched = true;
        }
      }
      // 降级：FIFO
      if (!matched) {
        pairByOrder(commands, log.toolResult ?? log.content, log.isError, log);
      }
    }
  }
  return commands;
}

/**
 * Group B: Agent 协议族（opencode / mobilecoder / mimo）
 *
 * - 单条 log_type=tool，content 含 description + output + status
 * - 由于后端 claudecode-style 工具信息存到 toolName/toolInputJson，
 *   而 opencode 等把一切都塞 content；本函数同时尝试两条路径。
 */
function extractAgentCommands(logs: LogEntry[]): CommandEntry[] {
  const result: CommandEntry[] = [];
  for (const log of logs) {
    if (log.type !== 'tool' || !isBashTool(log.toolName)) continue;
    const input = parseJsonSafe(log.toolInputJson);
    // opencode 把 input/output/status 放在 part.state
    const state = (input?.state as Record<string, unknown>) || input;
    const inner = (state?.input as Record<string, unknown>) || state;
    const command = (inner?.command as string) || (inner?.description as string) || '';
    const output = (state?.output as string) || log.content;
    const status = (state?.status as string) || '';
    result.push({
      id: log.toolCallId || `cmd-agent-${result.length}-${log.timestamp}`,
      toolName: log.toolName || 'bash',
      command,
      args: inner ? JSON.stringify(inner, null, 2) : undefined,
      output,
      success: status === 'success' || status === 'completed',
      timestamp: log.timestamp,
    });
  }
  return result;
}

/**
 * Group C-1: kimi
 *
 * - kimi 的 tool_input_json 是一段**字符串**，需要二次 JSON.parse。
 * - 工具名：Shell / Bash
 * - 成功判定：output 中是否含 "error" 关键字（kimi 无显式 success 字段）
 */
function extractKimiCommands(logs: LogEntry[]): CommandEntry[] {
  const commands: CommandEntry[] = [];
  for (const log of logs) {
    if (log.type === 'tool_call' && isBashTool(log.toolName)) {
      // kimi 二次解析：toolInputJson 是字符串化的 JSON 对象，
      // 直接 parse 一次就能拿到 command 字段；二次回退分支永远不可达（同一字符串）
      const inner = parseJsonSafe(log.toolInputJson);
      commands.push({
        id: log.toolCallId || `cmd-kimi-${commands.length}-${log.timestamp}`,
        toolName: log.toolName || 'Shell',
        command: (inner?.command as string) || log.content,
        args: inner ? JSON.stringify(inner, null, 2) : log.content,
        success: false,
        timestamp: log.timestamp,
      });
    } else if (log.type === 'tool_result') {
      let matched = false;
      if (log.toolCallId) {
        const cmd = commands.find(c => c.id === log.toolCallId);
        if (cmd) {
          cmd.output = log.toolResult ?? log.content;
          cmd.success = !/error/i.test(cmd.output || '');
          matched = true;
        }
      }
      if (!matched) {
        pairByOrder(commands, log.toolResult ?? log.content, undefined, log);
      }
    }
  }
  return commands;
}

/**
 * Group C-2: codewhale
 *
 * - tool_name 顶层：exec_shell
 * - input.command 提取命令
 * - tool_result.input.status 判定 success
 */
function extractCodeWhaleCommands(logs: LogEntry[]): CommandEntry[] {
  const commands: CommandEntry[] = [];
  for (const log of logs) {
    if (log.type === 'tool_call' && isBashTool(log.toolName)) {
      pushCodeWhaleCall(log, commands);
    } else if (log.type === 'tool_result') {
      applyCodeWhaleResult(log, commands);
    }
  }
  return commands;
}

/** codewhale 单条 tool_call → 推入新命令；input.command 提取命令 */
function pushCodeWhaleCall(log: LogEntry, commands: CommandEntry[]): void {
  const input = parseJsonSafe(log.toolInputJson) || parseJsonSafe(log.content);
  commands.push({
    id: log.toolCallId || `cmd-cw-${commands.length}-${log.timestamp}`,
    toolName: log.toolName || 'exec_shell',
    command: (input?.command as string) || '',
    args: input ? JSON.stringify(input, null, 2) : log.content,
    success: false,
    timestamp: log.timestamp,
  });
}

/** codewhale 成功判定在 tool_result.input.status === 'success' */
function applyCodeWhaleResult(log: LogEntry, commands: CommandEntry[]): void {
  const resultInput = parseJsonSafe(log.toolInputJson);
  const success = (resultInput?.status as string) === 'success';
  if (fillCodeWhaleByToolCallId(log, commands, success)) return;
  // FIFO 兜底：没 toolCallId 时按顺序配对
  const out = log.toolResult ?? log.content;
  pairByOrder(commands, out, !success, log);
}

function fillCodeWhaleByToolCallId(
  log: LogEntry,
  commands: CommandEntry[],
  success: boolean,
): boolean {
  if (!log.toolCallId) return false;
  const cmd = commands.find(c => c.id === log.toolCallId);
  if (!cmd) return false;
  cmd.output = log.toolResult ?? log.content;
  cmd.success = success;
  return true;
}

/**
 * Group D: codex
 *
 * - item.type === 'command_execution'
 * - item.command 可能是字符串数组（join(' && ')）
 * - item.aggregated_output / exit_code / duration_ms
 */
function extractCodexCommands(logs: LogEntry[]): CommandEntry[] {
  const commands: CommandEntry[] = [];
  for (const log of logs) {
    if (log.type === 'tool_call' && log.toolName === 'command_execution') {
      pushCodexCall(log, commands);
    } else if (log.type === 'tool_result') {
      applyCodexResult(log, commands);
    }
  }
  return commands;
}

/** codex 单条 tool_call → 推入新命令；command 可能是字符串数组，join 成 shell 复合语句 */
function pushCodexCall(log: LogEntry, commands: CommandEntry[]): void {
  const input = parseJsonSafe(log.toolInputJson) || parseJsonSafe(log.content);
  const rawCmd = input?.command;
  const command = Array.isArray(rawCmd) ? rawCmd.join(' && ') : (rawCmd as string) || '';
  commands.push({
    id: log.toolCallId || `cmd-codex-${commands.length}-${log.timestamp}`,
    toolName: 'command_execution',
    command,
    args: input ? JSON.stringify(input, null, 2) : log.content,
    success: false,
    timestamp: log.timestamp,
  });
}

/** 把 codex tool_result 的 exit_code / duration_ms 写回命中的命令 */
function applyCodexResult(log: LogEntry, commands: CommandEntry[]): void {
  // result input 里同时携带 exit_code / status / duration_ms，一次解析三处复用
  const resultInput = parseJsonSafe(log.toolInputJson);
  if (fillCodexByToolCallId(log, commands, resultInput)) return;
  // 兜底：没有 toolCallId 时按 FIFO 配对，并把 exit_code / duration_ms 补到最后一条上
  fillCodexByFifo(log, commands, resultInput);
}

function fillCodexByToolCallId(
  log: LogEntry,
  commands: CommandEntry[],
  resultInput: Record<string, unknown> | null,
): boolean {
  if (!log.toolCallId) return false;
  const cmd = commands.find(c => c.id === log.toolCallId);
  if (!cmd) return false;
  cmd.output = log.toolResult ?? log.content;
  cmd.exitCode = typeof resultInput?.exit_code === 'number' ? (resultInput.exit_code as number) : undefined;
  cmd.success = (resultInput?.status as string) === 'completed';
  if (typeof resultInput?.duration_ms === 'number') {
    cmd.durationMs = resultInput.duration_ms as number;
  }
  return true;
}

function fillCodexByFifo(
  log: LogEntry,
  commands: CommandEntry[],
  resultInput: Record<string, unknown> | null,
): void {
  const out = log.toolResult ?? log.content;
  const success = resultInput && (resultInput.status as string) === 'completed';
  const paired = pairByOrder(commands, out, !success, log);
  if (!paired) return;
  // FIFO 配到的就是最后一条，把 result input 里的 exit_code / duration_ms 补上去
  const last = commands[commands.length - 1];
  if (typeof resultInput?.exit_code === 'number') {
    last.exitCode = resultInput.exit_code as number;
  }
  if (typeof resultInput?.duration_ms === 'number') {
    last.durationMs = resultInput.duration_ms as number;
  }
}

/**
 * Group E: pi
 *
 * - tool_use / tool_result 配对
 * - 命令路径：toolExecution.args.command
 * - 结果：toolExecution.output / status / duration_ms
 */
function extractPiCommands(logs: LogEntry[]): CommandEntry[] {
  const commands: CommandEntry[] = [];
  for (const log of logs) {
    if ((log.type === 'tool_use' || log.type === 'tool_call') && isBashTool(log.toolName)) {
      pushPiCall(log, commands);
    } else if (log.type === 'tool_result') {
      applyPiResult(log, commands);
    }
  }
  return commands;
}

/** pi 单条 tool_use/tool_call → 推入新命令；command 路径在 toolExecution.args.command */
function pushPiCall(log: LogEntry, commands: CommandEntry[]): void {
  const input = parseJsonSafe(log.toolInputJson) || parseJsonSafe(log.content);
  const args = (input?.args as Record<string, unknown>) || input;
  commands.push({
    id: log.toolCallId || `cmd-pi-${commands.length}-${log.timestamp}`,
    toolName: log.toolName || 'bash',
    command: (args?.command as string) || (input?.command as string) || '',
    args: args ? JSON.stringify(args, null, 2) : log.content,
    success: false,
    timestamp: log.timestamp,
  });
}

/** 把 pi tool_result 的 output / status / duration_ms 写回命中的命令 */
function applyPiResult(log: LogEntry, commands: CommandEntry[]): void {
  const resultInput = parseJsonSafe(log.toolInputJson);
  if (fillPiByToolCallId(log, commands, resultInput)) return;
  fillPiByFifo(log, commands, resultInput);
}

function fillPiByToolCallId(
  log: LogEntry,
  commands: CommandEntry[],
  resultInput: Record<string, unknown> | null,
): boolean {
  if (!log.toolCallId) return false;
  const cmd = commands.find(c => c.id === log.toolCallId);
  if (!cmd) return false;
  cmd.output = (resultInput?.output as string) || log.toolResult || log.content;
  cmd.success = (resultInput?.status as string) === 'success';
  if (typeof resultInput?.duration_ms === 'number') {
    cmd.durationMs = resultInput.duration_ms as number;
  }
  return true;
}

function fillPiByFifo(
  log: LogEntry,
  commands: CommandEntry[],
  resultInput: Record<string, unknown> | null,
): void {
  const out = (resultInput?.output as string) || log.toolResult || log.content;
  const success = (resultInput?.status as string) === 'success';
  pairByOrder(commands, out, !success, log);
}

/**
 * Group F: atomcode（特殊：来源是 stderr，格式 `[tool→ bash args={...}]`）
 *
 * 当前后端会把 atomcode 解析后的 tool 写入 LogEntry，content 包含 stderr 行。
 * 这里同时尝试两种来源：
 * 1) content 里的 `[tool→ ...]` / `[tool← ...]` 正则
 * 2) toolName/toolInputJson（如果后端升级透出）
 */
function extractAtomcodeCommands(logs: LogEntry[]): CommandEntry[] {
  const commands: CommandEntry[] = [];
  // atomcode 的两条协议路径各自专用正则，避免在循环里重复创建
  const callRe = /\[tool→\s+(\w+)\s+args=(\{.*?\})\]/;
  const resultRe = /\[tool←\s+(\w+)\s+(OK|ERROR)\s+(\d+ms)?\]/;

  for (const log of logs) {
    // 路径 1：后端已透出 toolName / toolInputJson（升级后的协议）
    if (log.type === 'tool' && isBashTool(log.toolName)) {
      pushAtomToolCall(log, commands);
      continue;
    }
    // 路径 2：content 里的 `[tool→ ...]` / `[tool← ...]` stderr 风格
    const callMatch = log.content.match(callRe);
    if (callMatch) {
      pushAtomStderrCall(callMatch, log, commands);
      continue;
    }
    const resultMatch = log.content.match(resultRe);
    if (resultMatch) {
      applyAtomStderrResult(resultMatch, commands);
    }
  }
  return commands;
}

/** atomcode 路径 1：toolName 透出时直接组装 CommandEntry */
function pushAtomToolCall(log: LogEntry, commands: CommandEntry[]): void {
  const input = parseJsonSafe(log.toolInputJson);
  commands.push({
    id: `cmd-atom-${commands.length}-${log.timestamp}`,
    toolName: log.toolName || 'bash',
    command: (input?.command as string) || '',
    args: input ? JSON.stringify(input, null, 2) : log.content,
    success: false,
    timestamp: log.timestamp,
  });
}

/** atomcode 路径 2：从 stderr 风格的 `[tool→ ... args={...}]` 行提取命令 */
function pushAtomStderrCall(callMatch: RegExpMatchArray, log: LogEntry, commands: CommandEntry[]): void {
  const [, toolName, argsJson] = callMatch;
  const args = parseJsonSafe(argsJson);
  commands.push({
    id: `cmd-atom-${commands.length}-${log.timestamp}`,
    toolName,
    command: (args?.command as string) || '',
    args: argsJson,
    success: false,
    timestamp: log.timestamp,
  });
}

/** atomcode 路径 2：从 `[tool← ... OK|ERROR Nms]` 行把状态写回最近同名的未完成命令 */
function applyAtomStderrResult(resultMatch: RegExpMatchArray, commands: CommandEntry[]): void {
  const [, toolName, status, duration] = resultMatch;
  // 倒序找最近一个同名且 output 未填的命令，避免误填到早期同名调用
  for (let i = commands.length - 1; i >= 0; i--) {
    if (commands[i].toolName === toolName && commands[i].output === undefined) {
      commands[i].success = status === 'OK';
      if (duration) {
        const ms = parseDuration(duration);
        if (ms != null) commands[i].durationMs = ms;
      }
      break;
    }
  }
}

/** 解析 "39ms" / "1.2s" 为毫秒数。返回 null 表示解析失败。 */
function parseDuration(text: string): number | null {
  const trimmed = text.trim();
  const m = /^(\d+(?:\.\d+)?)(ms|s)$/.exec(trimmed);
  if (!m) return null;
  const value = parseFloat(m[1]);
  return m[2] === 's' ? value * 1000 : value;
}

/**
 * Group G: hermes
 *
 * 纯文本流，没有结构化工具调用 — 始终返回空数组。
 * 调用方应据此展示"不支持"的友好提示。
 */
function extractHermesCommands(_logs: LogEntry[]): CommandEntry[] {
  return [];
}

// ─── 公共分派函数 ─────────────────────────────────────────────

/**
 * 按执行器名分派到对应的提取器。
 *
 * 未知执行器名时回退到 `extractClaudeCommands`（最常见的协议族），
 * 行为安全且不会抛错。
 */
export function extractCommandsByExecutor(
  logs: LogEntry[],
  executor: string | null | undefined,
): CommandEntry[] {
  const name = (executor || '').toLowerCase();
  switch (name) {
    case 'claudecode':
    case 'claude_code':
    case 'claude':
    case 'codebuddy':
      return extractClaudeCommands(logs);
    case 'opencode':
    case 'mobilecoder':
    case 'mimo':
      return extractAgentCommands(logs);
    case 'kimi':
      return extractKimiCommands(logs);
    case 'codewhale':
      return extractCodeWhaleCommands(logs);
    case 'codex':
      return extractCodexCommands(logs);
    case 'pi':
      return extractPiCommands(logs);
    case 'atomcode':
    case 'atom':
      return extractAtomcodeCommands(logs);
    case 'hermes':
      return extractHermesCommands(logs);
    default:
      // 未知执行器：尝试 Claude 协议族（最常见），命中 0 条不报错
      return extractClaudeCommands(logs);
  }
}

/** 提取器在 Playwright 单元测试中用于验证 */
export const __test__ = {
  extractClaudeCommands,
  extractAgentCommands,
  extractKimiCommands,
  extractCodeWhaleCommands,
  extractCodexCommands,
  extractPiCommands,
  extractAtomcodeCommands,
  parseDuration,
};
