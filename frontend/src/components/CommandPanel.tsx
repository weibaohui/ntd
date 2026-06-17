/**
 * issue #648: 命令视图面板
 *
 * 从 `LogEntry[]` 中按执行器协议提取 CommandEntry 列表，并渲染为
 * 可折叠的命令卡片。每条命令展示：
 * - `$ command` 标题（等宽字体 + 复制按钮）
 * - output（默认折叠，长输出只显示前 100 字符预览）
 * - 时长 / 状态徽章
 *
 * 设计取舍：
 * - 提取与渲染分离（commandExtractor 工具），便于复用与单测。
 * - 长 output 默认折叠：AI 命令输出常常上千行，渲染全开会卡顿。
 * - hermes 执行器显式提示"不支持命令提取"，而不是悄悄返回空数组。
 */
import { useMemo } from 'react';
import { Empty, Tooltip, Tag, Alert } from 'antd';
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
      <Empty
        description="本次执行未捕获到可提取的 Bash 命令"
        image={Empty.PRESENTED_IMAGE_SIMPLE}
      />
    );
  }

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }} data-testid="command-panel">
      <div style={{ fontSize: 11, color: 'var(--color-text-tertiary)', marginBottom: 4 }}>
        共 {commands.length} 条命令
        <Tooltip title="按执行器协议逐条提取；跨调用-返回的关联失败时会按时间顺序配对">
          <Tag style={{ marginLeft: 8 }} color="default">i</Tag>
        </Tooltip>
      </div>
      {commands.map((cmd, idx) => (
        <CommandCard key={cmd.id || `cmd-${idx}`} command={cmd} index={idx} />
      ))}
    </div>
  );
}

export { CommandCard };
