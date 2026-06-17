/**
 * issue #648 — commandExtractor 单元/集成测试
 *
 * 通过 Vite dev server 加载真实的 commandExtractor.ts 模块，
 * 在浏览器中执行各执行器分支的提取逻辑，覆盖：
 * - parseJsonSafe 的容错
 * - isBashTool 的大小写无关比较
 * - Claude / Agent / kimi / codex / pi / atomcode 的提取正确性
 * - extractCommandsByExecutor 的分派
 *
 * Playwright 跑在 headless 浏览器中，因此 Vite dev server（npm run dev，5173 端口）
 * 必须先启动。失败时打印详细 diff。
 */

import { test, expect } from '@playwright/test';

// 与 playwright.config.ts 的 baseURL 保持一致（Vite dev server 默认 5173）。
// 5173 = `npm run dev`（Vite 单独），spec 只需 Vite 暴露 `/src/*` 即可加载 ESM 模块。
// `make dev` 的 18088（Vite + Rust embedded）也可，但启动更重；本 spec 选 5173 走轻量路径。
const DEV_URL = process.env.E2E_BASE_URL || 'http://localhost:5173';

test.describe('commandExtractor — Issue #648', () => {
  test('parseJsonSafe 应在非法 JSON 上返回 null', async ({ page }) => {
    await page.goto(DEV_URL);
    const result = await page.evaluate(async () => {
      const mod = await import('/src/utils/commandExtractor.ts');
      return {
        ok1: mod.parseJsonSafe('{"a":1}'),
        nullOnEmpty: mod.parseJsonSafe(''),
        nullOnNull: mod.parseJsonSafe(null),
        nullOnGarbage: mod.parseJsonSafe('not-json'),
        nullOnArray: mod.parseJsonSafe('[1,2]'),
      };
    });
    expect(result.ok1).toEqual({ a: 1 });
    expect(result.nullOnEmpty).toBeNull();
    expect(result.nullOnNull).toBeNull();
    expect(result.nullOnGarbage).toBeNull();
    expect(result.nullOnArray).toBeNull();
  });

  test('isBashTool 应大小写无关地匹配 Bash / bash / Shell / exec_shell', async ({ page }) => {
    await page.goto(DEV_URL);
    const result = await page.evaluate(async () => {
      const mod = await import('/src/utils/commandExtractor.ts');
      return {
        bash: mod.isBashTool('Bash'),
        lower: mod.isBashTool('bash'),
        shell: mod.isBashTool('Shell'),
        exec: mod.isBashTool('exec_shell'),
        none: mod.isBashTool('Read'),
        empty: mod.isBashTool(''),
        undef: mod.isBashTool(undefined),
      };
    });
    expect(result.bash).toBe(true);
    expect(result.lower).toBe(true);
    expect(result.shell).toBe(true);
    expect(result.exec).toBe(true);
    expect(result.none).toBe(false);
    expect(result.empty).toBe(false);
    expect(result.undef).toBe(false);
  });

  test('Claude 协议族应按 toolCallId 配对 command 与 output', async ({ page }) => {
    await page.goto(DEV_URL);
    const result = await page.evaluate(async () => {
      const mod = await import('/src/utils/commandExtractor.ts');
      const logs = [
        { timestamp: 't1', type: 'tool_use', content: 'x', toolName: 'Bash', toolInputJson: JSON.stringify({ command: 'ls -la' }), toolCallId: 'a1' },
        { timestamp: 't2', type: 'tool_result', content: 'file.txt\ndir/', toolCallId: 'a1', isError: false },
        { timestamp: 't3', type: 'tool_use', content: 'x', toolName: 'Bash', toolInputJson: JSON.stringify({ command: 'git status' }), toolCallId: 'a2' },
        { timestamp: 't4', type: 'tool_result', content: 'fatal: not a git repo', toolCallId: 'a2', isError: true },
      ];
      return mod.__test__.extractClaudeCommands(logs);
    });
    expect(result).toHaveLength(2);
    expect(result[0].command).toBe('ls -la');
    expect(result[0].output).toBe('file.txt\ndir/');
    expect(result[0].success).toBe(true);
    expect(result[1].command).toBe('git status');
    expect(result[1].success).toBe(false);
  });

  test('Agent 协议族应从 state.input.command + state.output 提取', async ({ page }) => {
    await page.goto(DEV_URL);
    const result = await page.evaluate(async () => {
      const mod = await import('/src/utils/commandExtractor.ts');
      const logs = [
        {
          timestamp: 't1', type: 'tool', content: '执行 bash',
          toolName: 'bash',
          toolInputJson: JSON.stringify({
            state: { status: 'success', input: { command: 'ls' }, output: 'a.txt' },
          }),
        },
      ];
      return mod.__test__.extractAgentCommands(logs);
    });
    expect(result).toHaveLength(1);
    expect(result[0].command).toBe('ls');
    expect(result[0].output).toBe('a.txt');
    expect(result[0].success).toBe(true);
  });

  test('kimi 应二次解析 stringified arguments', async ({ page }) => {
    await page.goto(DEV_URL);
    const result = await page.evaluate(async () => {
      const mod = await import('/src/utils/commandExtractor.ts');
      // kimi 把 arguments 序列化成字符串再放到 toolInputJson
      const logs = [
        {
          timestamp: 't1', type: 'tool_call', content: 'x', toolName: 'Shell',
          toolInputJson: JSON.stringify({ command: 'pwd' }),
        },
        { timestamp: 't2', type: 'tool_result', content: '/home/user', toolCallId: undefined, isError: false },
      ];
      return mod.__test__.extractKimiCommands(logs);
    });
    expect(result).toHaveLength(1);
    expect(result[0].command).toBe('pwd');
    expect(result[0].output).toBe('/home/user');
  });

  test('codewhale 应按 exec_shell + status=success 判定结果', async ({ page }) => {
    await page.goto(DEV_URL);
    const result = await page.evaluate(async () => {
      const mod = await import('/src/utils/commandExtractor.ts');
      const logs = [
        {
          timestamp: 't1', type: 'tool_call', content: 'x', toolName: 'exec_shell',
          toolCallId: 'cw1',
          toolInputJson: JSON.stringify({ command: 'pwd' }),
        },
        {
          timestamp: 't2', type: 'tool_result', content: '/home/user', toolCallId: 'cw1',
          toolInputJson: JSON.stringify({ status: 'success' }),
        },
      ];
      return mod.__test__.extractCodeWhaleCommands(logs);
    });
    expect(result).toHaveLength(1);
    expect(result[0].command).toBe('pwd');
    expect(result[0].output).toBe('/home/user');
    expect(result[0].success).toBe(true);
  });

  test('codex 应支持字符串数组形式的 command', async ({ page }) => {
    await page.goto(DEV_URL);
    const result = await page.evaluate(async () => {
      const mod = await import('/src/utils/commandExtractor.ts');
      const logs = [
        {
          timestamp: 't1', type: 'tool_call', content: 'x', toolName: 'command_execution',
          toolInputJson: JSON.stringify({ command: ['git add .', 'git commit -m test'] }),
        },
        {
          timestamp: 't2', type: 'tool_result', content: 'committed', toolCallId: undefined,
          toolInputJson: JSON.stringify({ exit_code: 0, status: 'completed', duration_ms: 123 }),
        },
      ];
      return mod.__test__.extractCodexCommands(logs);
    });
    expect(result).toHaveLength(1);
    expect(result[0].command).toBe('git add . && git commit -m test');
    expect(result[0].exitCode).toBe(0);
    expect(result[0].durationMs).toBe(123);
    expect(result[0].success).toBe(true);
  });

  test('codex FIFO 配对结果乱序时 exit_code / duration_ms 应写到真正命中的命令', async ({ page }) => {
    // 回归测试：3 条命令依次 push；result 没有 toolCallId，按 FIFO 命中第一条未填的 cmd；
    // 修复前会错误写到 commands[length-1]（最后一条），修复后应写到第 1 条。
    await page.goto(DEV_URL);
    const result = await page.evaluate(async () => {
      const mod = await import('/src/utils/commandExtractor.ts');
      const logs = [
        { timestamp: 't1', type: 'tool_call', content: 'x', toolName: 'command_execution',
          toolInputJson: JSON.stringify({ command: 'echo first' }) },
        { timestamp: 't2', type: 'tool_call', content: 'x', toolName: 'command_execution',
          toolInputJson: JSON.stringify({ command: 'echo middle' }) },
        { timestamp: 't3', type: 'tool_call', content: 'x', toolName: 'command_execution',
          toolInputJson: JSON.stringify({ command: 'echo last' }) },
        { timestamp: 't4', type: 'tool_result', content: 'first-output', toolCallId: undefined,
          toolInputJson: JSON.stringify({ exit_code: 0, status: 'completed', duration_ms: 50 }) },
      ];
      return mod.__test__.extractCodexCommands(logs);
    });
    expect(result).toHaveLength(3);
    // FIFO 命中第一条（FIFO 配对只看 output 是否为空，与 push 顺序一致）
    expect(result[0].output).toBe('first-output');
    expect(result[0].exitCode).toBe(0);
    expect(result[0].durationMs).toBe(50);
    expect(result[0].success).toBe(true);
    // 中间与最后一条不应被错误填充
    expect(result[1].output).toBeUndefined();
    expect(result[2].output).toBeUndefined();
  });

  test('pi 应从 toolExecution.args.command 提取', async ({ page }) => {
    await page.goto(DEV_URL);
    const result = await page.evaluate(async () => {
      const mod = await import('/src/utils/commandExtractor.ts');
      const logs = [
        {
          timestamp: 't1', type: 'tool_use', content: 'x', toolName: 'bash',
          toolInputJson: JSON.stringify({ args: { command: 'echo hi' } }),
        },
        {
          timestamp: 't2', type: 'tool_result', content: 'x', toolCallId: undefined,
          toolInputJson: JSON.stringify({ output: 'hi', status: 'success' }),
        },
      ];
      return mod.__test__.extractPiCommands(logs);
    });
    expect(result).toHaveLength(1);
    expect(result[0].command).toBe('echo hi');
    expect(result[0].output).toBe('hi');
    expect(result[0].success).toBe(true);
  });

  test('pi FIFO 配对：tool_result 无 toolCallId 时按顺序填到未填的 cmd', async ({ page }) => {
    // 覆盖 applyPiResult 的 fillPiByFifo 分支（PR #656 评审 MEDIUM #2 缺漏）。
    // pushPiCall 不带 toolCallId → cmd.id = cmd-pi-N-...；
    // applyPiResult 的 result 也不带 toolCallId → 走 FIFO 兜底，命中第一条 output 未填的 cmd。
    await page.goto(DEV_URL);
    const result = await page.evaluate(async () => {
      const mod = await import('/src/utils/commandExtractor.ts');
      const logs = [
        {
          timestamp: 't1', type: 'tool_use', content: 'x', toolName: 'bash',
          // 无 toolCallId：让 pushPiCall 走自动生成 id 路径
          toolInputJson: JSON.stringify({ args: { command: 'echo first' } }),
        },
        {
          timestamp: 't2', type: 'tool_use', content: 'x', toolName: 'bash',
          toolInputJson: JSON.stringify({ args: { command: 'echo second' } }),
        },
        {
          timestamp: 't3', type: 'tool_result', content: 'x', toolCallId: undefined,
          // 无 toolCallId：让 applyPiResult 走 fillPiByFifo 分支
          toolInputJson: JSON.stringify({ output: 'first-output', status: 'success', duration_ms: 77 }),
        },
      ];
      return mod.__test__.extractPiCommands(logs);
    });
    expect(result).toHaveLength(2);
    // FIFO 命中第一条
    expect(result[0].command).toBe('echo first');
    expect(result[0].output).toBe('first-output');
    expect(result[0].success).toBe(true);
    // durationMs 只在 fillPiByToolCallId 路径设置，fillPiByFifo 走 pairByOrder
    // 不携带 duration_ms，所以这里是 undefined（与 fillPiByFifo 的实现契约一致）
    expect(result[0].durationMs).toBeUndefined();
    // 第二条不应被错误填充
    expect(result[1].output).toBeUndefined();
    expect(result[1].success).toBe(false);
  });

  test('kimi toolCallId 成功判定：output 含 error 时 success=false，否则 true', async ({ page }) => {
    // 覆盖 extractKimiCommands 的 toolCallId 分支 success 判定
    // （PR #656 评审 MEDIUM #2 缺漏）。
    // 判定规则（commandExtractor.ts:174）：`cmd.success = !/error/i.test(cmd.output || '')`。
    await page.goto(DEV_URL);
    const result = await page.evaluate(async () => {
      const mod = await import('/src/utils/commandExtractor.ts');
      // 3 条命令 + 3 条 result，分别用 toolCallId 对齐，混合正常 / 错误 / 边界输出
      const logs = [
        // case 1: 正常输出
        { timestamp: 't1', type: 'tool_call', content: 'x', toolName: 'Shell',
          toolInputJson: JSON.stringify({ command: 'pwd' }),
          toolCallId: 'k1' },
        { timestamp: 't2', type: 'tool_result', content: '/home/user', toolCallId: 'k1', isError: false },
        // case 2: output 含 error 子串 → success=false
        { timestamp: 't3', type: 'tool_call', content: 'x', toolName: 'Shell',
          toolInputJson: JSON.stringify({ command: 'git status' }),
          toolCallId: 'k2' },
        { timestamp: 't4', type: 'tool_result', content: 'Error: not a git repo', toolCallId: 'k2', isError: true },
        // case 3: output 大小写混合的 ERROR（regex /error/i 大小写无关）
        { timestamp: 't5', type: 'tool_call', content: 'x', toolName: 'Shell',
          toolInputJson: JSON.stringify({ command: 'unknown-cmd' }),
          toolCallId: 'k3' },
        { timestamp: 't6', type: 'tool_result', content: 'command not found', toolCallId: 'k3', isError: true },
      ];
      return mod.__test__.extractKimiCommands(logs);
    });
    expect(result).toHaveLength(3);
    // 正常：不含 error → success=true
    expect(result[0].command).toBe('pwd');
    expect(result[0].output).toBe('/home/user');
    expect(result[0].success).toBe(true);
    // 含 Error → success=false
    expect(result[1].command).toBe('git status');
    expect(result[1].output).toBe('Error: not a git repo');
    expect(result[1].success).toBe(false);
    // isError=true 但 output 不含 error → 仍判 success=true（kimi 信任 output 文本）
    expect(result[2].command).toBe('unknown-cmd');
    expect(result[2].output).toBe('command not found');
    expect(result[2].success).toBe(true);
  });

  test('atomcode 应能从 stderr 风格 content 解析 [tool→ / [tool←', async ({ page }) => {
    await page.goto(DEV_URL);
    const result = await page.evaluate(async () => {
      const mod = await import('/src/utils/commandExtractor.ts');
      const logs = [
        { timestamp: 't1', type: 'stderr', content: '[tool→ bash args={"command":"ls -la"}]' },
        { timestamp: 't2', type: 'stderr', content: '[tool← bash OK 39ms]\nfile.txt' },
      ];
      return mod.__test__.extractAtomcodeCommands(logs);
    });
    expect(result).toHaveLength(1);
    expect(result[0].command).toBe('ls -la');
    expect(result[0].success).toBe(true);
    expect(result[0].durationMs).toBe(39);
    // stderr 行 `[tool← ...]` 前缀之后的剩余内容（命令实际输出）不应被吞掉
    expect(result[0].output).toBe('file.txt');
  });

  test('extractCommandsByExecutor 应正确分派到各协议族', async ({ page }) => {
    await page.goto(DEV_URL);
    const result = await page.evaluate(async () => {
      const mod = await import('/src/utils/commandExtractor.ts');
      const logs = [
        { timestamp: 't1', type: 'tool', content: 'x', toolName: 'bash',
          toolInputJson: JSON.stringify({ state: { status: 'success', input: { command: 'ls' }, output: 'a' } }) },
      ];
      return {
        opencode: mod.extractCommandsByExecutor(logs, 'opencode').length,
        mobilecoder: mod.extractCommandsByExecutor(logs, 'mobilecoder').length,
        mimo: mod.extractCommandsByExecutor(logs, 'mimo').length,
        hermes: mod.extractCommandsByExecutor(logs, 'hermes').length,
        unknown: mod.extractCommandsByExecutor(logs, 'something-new').length,
      };
    });
    expect(result.opencode).toBe(1);
    expect(result.mobilecoder).toBe(1);
    expect(result.mimo).toBe(1);
    expect(result.hermes).toBe(0);
    // 未知执行器走 Claude fallback — 上面那条日志不是 Claude 协议，应是 0
    expect(result.unknown).toBe(0);
  });
});
