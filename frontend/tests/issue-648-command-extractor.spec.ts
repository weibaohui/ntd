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
 * Playwright 跑在 headless 浏览器中，因此 Vite dev server 必须先启动
 * （在 18088 端口，ntd 项目的 dev 默认端口）。失败时打印详细 diff。
 */

import { test, expect } from '@playwright/test';

const DEV_URL = process.env.E2E_BASE_URL || 'http://localhost:18089';

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
