/**
 * issue #648: 命令视图面板
 *
 * 从 `LogEntry[]` 中按执行器协议提取 CommandEntry 列表，并渲染为
 * 可折叠的命令卡片。终端风格 UI，适配亮色/暗色主题。
 *
 * 设计取舍：
 * - 提取与渲染分离（commandExtractor 工具），便于复用与单测。
 * - 长 output 默认折叠：AI 命令输出常常上千行，渲染全开会卡顿。
 * - hermes 执行器显式提示"不支持命令提取"，而不是悄悄返回空数组。
 */
import { useMemo } from 'react';
import { Alert } from 'antd';
import { CodeOutlined, InfoCircleOutlined } from '@ant-design/icons';
import type { LogEntry, CommandEntry } from '@/types';
import { extractCommandsByExecutor } from '@/utils/commandExtractor';
import { CommandCard } from './CommandCard';

export interface CommandPanelProps {
  logs: LogEntry[];
  executor: string | null | undefined;
}

/** Panel 入口：分派提取 → 渲染 */
export function CommandPanel({ logs, executor }: CommandPanelProps) {
  const commands = useMemo<CommandEntry[]>(
    () => extractCommandsByExecutor(logs, executor),
    [logs, executor],
  );

  if (executor && executor.toLowerCase() === 'hermes') {
    return (
      <Alert
        type="info"
        showIcon
        message="Hermes 执行器无结构化工具调用日志，暂不支持命令提取"
        style={{ marginBottom: 8 }}
      />
    );
  }

  if (commands.length === 0) {
    return (
      // role="status" 隐式 aria-live="polite"，无需重复声明；
      // 取代之前 antd Empty 内置的 role="status" 行为
      <div
        role="status"
        style={{
          display: 'flex',
          flexDirection: 'column',
          alignItems: 'center',
          justifyContent: 'center',
          padding: '48px 24px',
          gap: 12,
        }}
      >
        <CodeOutlined style={{ fontSize: 32, color: 'var(--color-text-tertiary)' }} />
        <div style={{
          fontSize: 14,
          color: 'var(--color-text-secondary)',
          fontWeight: 500,
        }}>
          未捕获到可提取的 Bash 命令
        </div>
        <div style={{
          fontSize: 12,
          color: 'var(--color-text-tertiary)',
          textAlign: 'center',
          maxWidth: 320,
        }}>
          本次执行未产生 Bash / Shell 类工具调用，或日志格式不兼容当前提取器
        </div>
      </div>
    );
  }

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }} data-testid="command-panel">
      {/* 统计栏 */}
      <div style={{
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'space-between',
        padding: '6px 12px',
        borderRadius: 8,
        background: 'var(--color-bg)',
        border: '1px solid var(--color-border-light)',
        fontSize: 12,
        color: 'var(--color-text-tertiary)',
      }}>
        <span style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
          <CodeOutlined />
          <span style={{ fontWeight: 500, color: 'var(--color-text-secondary)' }}>
            共 {commands.length} 条命令
          </span>
        </span>
        <span style={{
          display: 'flex',
          alignItems: 'center',
          gap: 4,
          fontSize: 11,
        }}>
          <InfoCircleOutlined />
          按时间顺序配对
        </span>
      </div>
      {/* 命令卡片列表 */}
      {commands.map((cmd, idx) => (
        <CommandCard key={cmd.id || `cmd-${idx}`} command={cmd} index={idx} />
      ))}
    </div>
  );
}

export { CommandCard };
