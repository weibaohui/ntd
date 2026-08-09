// CommandPanel 单元测试（命令视图，issue #648）。
//
// 本文件由原 Playwright harness spec `tests/issue-648-command-view-ui.spec.ts` 迁移而来：
// 原写法用 vite dev server 服务 `/tests/issue-648-mount.html`（其引用的 mount 脚本
// issue-648-mount.ts 甚至已丢失），把 CommandPanel 挂到浏览器再断言 body 文本——既依赖
// 独立的 5173 vite 进程（make dev 的 18088 embedded 不服务 /tests/*），又因 mount 脚本缺失
// 而必定超时失败。组件本身是纯渲染（提取逻辑已抽到 commandExtractor 并有单测覆盖），
// 直接用 @testing-library/react 渲染断言即可，无需浏览器与 vite。
//
// 覆盖点（与原 spec 对应）：正常日志渲染命令卡片+计数+成功/失败标签、hermes 不支持提示、
// 空日志空态。

import { describe, it, expect } from 'vitest';
import { render } from '@testing-library/react';
import { CommandPanel } from './CommandPanel';
import type { LogEntry } from '@/types';

// 三条命令：两条成功、一条失败，覆盖成功/失败两类标签。
const SAMPLE_LOGS: LogEntry[] = [
  { timestamp: '2024-01-01T10:00:00Z', type: 'tool_use', content: 'x', toolName: 'Bash', toolInputJson: JSON.stringify({ command: 'git pull origin main' }), toolCallId: 'c1' },
  { timestamp: '2024-01-01T10:00:01Z', type: 'tool_result', content: 'Already up to date.', toolCallId: 'c1', isError: false },
  { timestamp: '2024-01-01T10:00:02Z', type: 'tool_use', content: 'x', toolName: 'Bash', toolInputJson: JSON.stringify({ command: 'npm install' }), toolCallId: 'c2' },
  { timestamp: '2024-01-01T10:00:08Z', type: 'tool_result', content: 'added 120 packages in 6s', toolCallId: 'c2', isError: false },
  { timestamp: '2024-01-01T10:00:09Z', type: 'tool_use', content: 'x', toolName: 'Bash', toolInputJson: JSON.stringify({ command: 'docker build .' }), toolCallId: 'c3' },
  { timestamp: '2024-01-01T10:00:13Z', type: 'tool_result', content: 'ERROR: build failed\ndockerfile:10:2 unknown instruction', toolCallId: 'c3', isError: true },
];

describe('CommandPanel', () => {
  it('渲染命令卡片：3 条命令均展示，含计数与成功/失败标签', () => {
    const { container } = render(<CommandPanel logs={SAMPLE_LOGS} executor="claudecode" />);

    // 3 条命令都应该被提取并展示
    expect(container.textContent).toContain('git pull origin main');
    expect(container.textContent).toContain('npm install');
    expect(container.textContent).toContain('docker build .');
    // 共 N 条命令的统计
    expect(container.textContent).toContain('共 3 条命令');
    // 成功 / 失败 标签都要出现（CommandCard 按 command.success 渲染「成功」/「失败」）
    expect(container.textContent).toContain('成功');
    expect(container.textContent).toContain('失败');
  });

  it('hermes 执行器显示「不支持」提示，不走命令提取', () => {
    // hermes 显式提示不支持，而非悄悄返回空数组（设计取舍见组件头注释）
    const { container } = render(<CommandPanel logs={[]} executor="hermes" />);
    expect(container.textContent).toContain('Hermes');
    expect(container.textContent).toContain('不支持');
  });

  it('空日志显示「未捕获到」空态', () => {
    const { container } = render(<CommandPanel logs={[]} executor="claudecode" />);
    expect(container.textContent).toContain('未捕获到');
  });
});
