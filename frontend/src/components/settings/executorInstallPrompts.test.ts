import { describe, it, expect } from 'vitest';
import {
  INSTALL_EXECUTOR_ACTION_TYPE,
  INSTALL_CLAUCODE_ACTION_KEY,
  INSTALL_CLAUCODE_PROMPT,
  getExecutorInstallPrompt,
  getInstallableExecutorNames,
} from './executorInstallPrompts';

/**
 * 验证 getExecutorInstallPrompt 对已知执行器返回正确结构。
 * 这是该文件最核心的查找函数，覆盖正常路径。
 */
describe('getExecutorInstallPrompt', () => {
  it('returns prompt and actionKey for claudecode', () => {
    const result = getExecutorInstallPrompt('claudecode');
    expect(result).not.toBeNull();
    expect(result?.actionKey).toBe(INSTALL_CLAUCODE_ACTION_KEY);
    expect(result?.prompt).toBe(INSTALL_CLAUCODE_PROMPT);
  });

  it('returns null for unknown executor', () => {
    const result = getExecutorInstallPrompt('nonexistent');
    expect(result).toBeNull();
  });
});

/**
 * 验证所有可安装执行器都有完整的 prompt 与 actionKey。
 * 避免新增执行器时遗漏安装提示词。
 */
describe('installable executors', () => {
  it('contains at least the core executors', () => {
    const names = getInstallableExecutorNames();
    expect(names).toContain('claudecode');
    expect(names).toContain('codex');
    expect(names).toContain('kimi');
    expect(names).toContain('zhanlu');
  });

  it('every installable executor has non-empty prompt and actionKey', () => {
    const names = getInstallableExecutorNames();
    expect(names.length).toBeGreaterThan(0);
    for (const name of names) {
      const result = getExecutorInstallPrompt(name);
      expect(result).not.toBeNull();
      expect(result?.actionKey).toBe(name);
      expect(result?.prompt.length).toBeGreaterThan(50);
    }
  });
});

/**
 * 验证 prompt 内容包含必要的操作系统检测与验证指令。
 * 这些关键字是 AI 正确执行安装的前提。
 * 遍历所有可安装执行器，确保每个 prompt 都包含。
 */
describe('prompt content', () => {
  it('every prompt mentions macOS, Linux and Windows', () => {
    const names = getInstallableExecutorNames();
    for (const name of names) {
      const result = getExecutorInstallPrompt(name);
      expect(result, `${name}: prompt should not be null`).not.toBeNull();
      expect(result!.prompt, `${name}: should mention macOS`).toContain('macOS');
      expect(result!.prompt, `${name}: should mention Linux`).toContain('Linux');
      expect(result!.prompt, `${name}: should mention Windows`).toContain('Windows');
    }
  });

  it('every prompt asks to verify version with --version', () => {
    const names = getInstallableExecutorNames();
    for (const name of names) {
      const result = getExecutorInstallPrompt(name);
      expect(result, `${name}: prompt should not be null`).not.toBeNull();
      expect(result!.prompt, `${name}: should contain --version`).toContain('--version');
    }
  });

  it('every prompt mentions an install command or script', () => {
    const names = getInstallableExecutorNames();
    for (const name of names) {
      const result = getExecutorInstallPrompt(name);
      expect(result, `${name}: prompt should not be null`).not.toBeNull();
      // 每个 prompt 应该至少包含一个 ` 包裹的 shell 命令，说明给了具体的安装指令
      expect(result!.prompt, `${name}: should contain a shell command`).toMatch(/`[^`]+`/);
    }
  });

  it('claudecode prompt asks to verify specifically claude --version', () => {
    expect(INSTALL_CLAUCODE_PROMPT).toContain('claude --version');
  });

  it('uses the shared action type', () => {
    expect(typeof INSTALL_EXECUTOR_ACTION_TYPE).toBe('string');
    expect(INSTALL_EXECUTOR_ACTION_TYPE).toBe('install_executor');
  });
});
