/**
 * issue #648: 单条命令卡片
 *
 * - 标题：`$ command`，等宽字体，左侧带状态色边框
 * - 工具名小标签（Bash / bash / exec_shell / ...）
 * - 复制按钮：把整条 command 文本写入剪贴板
 * - output 区域：默认折叠；点击展开/收起
 * - 长 output 默认截断预览（>300 字符）
 */
import { useState } from 'react';
import { Button, Tooltip, message } from 'antd';
import { CopyOutlined, CheckCircleOutlined, CloseCircleOutlined, ClockCircleOutlined, DownOutlined, RightOutlined } from '@ant-design/icons';
import type { CommandEntry } from '@/types';
import { copyToClipboard } from '@/utils/clipboard';

const OUTPUT_PREVIEW_LIMIT = 300;

export interface CommandCardProps {
  command: CommandEntry;
  index: number;
}

export function CommandCard({ command, index }: CommandCardProps) {
  // 长 output 默认折叠，避免千行输出让单卡撑爆 viewport
  const hasOutput = !!command.output && command.output.length > 0;
  const isLong = hasOutput && command.output!.length > OUTPUT_PREVIEW_LIMIT;
  const [expanded, setExpanded] = useState(!isLong);

  const onCopy = async () => {
    const ok = await copyToClipboard(command.command);
    message[ok ? 'success' : 'error'](ok ? '已复制命令' : '复制失败');
  };

  const borderColor = command.success ? 'var(--color-success)' : 'var(--color-error)';
  const StatusIcon = command.success ? <CheckCircleOutlined /> : <CloseCircleOutlined />;
  const statusText = command.success ? '成功' : '失败';

  // 折叠状态下只展示前 N 字符 + "..."
  const displayedOutput = hasOutput && !expanded
    ? `${command.output!.slice(0, OUTPUT_PREVIEW_LIMIT)}…`
    : command.output || '';

  return (
    <div
      data-testid={`command-card-${index}`}
      style={{
        border: `1px solid var(--color-border-light)`,
        borderLeft: `3px solid ${borderColor}`,
        borderRadius: 6,
        background: 'var(--color-bg-elevated)',
        padding: '8px 12px',
      }}
    >
      {/* 标题行：状态条 + 工具名 + $ command + 复制按钮 + 时长 */}
      <div style={{ display: 'flex', alignItems: 'center', gap: 8, flexWrap: 'wrap' }}>
        <span style={{ color: borderColor, fontSize: 12 }}>{StatusIcon}</span>
        <span style={{
          fontSize: 10, padding: '1px 6px', borderRadius: 3,
          background: 'var(--color-border-light)', color: 'var(--color-text-secondary)',
        }}>{command.toolName}</span>
        <code
          data-testid={`command-text-${index}`}
          style={{
            flex: 1, minWidth: 0,
            fontFamily: 'var(--font-mono)', fontSize: 12,
            color: 'var(--color-text)', overflow: 'hidden',
            textOverflow: 'ellipsis', whiteSpace: 'nowrap',
          }}
        >
          $ {command.command || '(空命令)'}
        </code>
        <Tooltip title="复制命令">
          <Button
            type="text" size="small" icon={<CopyOutlined />}
            onClick={onCopy}
            aria-label="复制命令"
            data-testid={`command-copy-${index}`}
          />
        </Tooltip>
        {command.durationMs != null && (
          <Tooltip title={`耗时 ${command.durationMs}ms`}>
            <span style={{ fontSize: 11, color: 'var(--color-text-tertiary)', display: 'inline-flex', alignItems: 'center', gap: 2 }}>
              <ClockCircleOutlined /> {formatDuration(command.durationMs)}
            </span>
          </Tooltip>
        )}
        <span style={{
          fontSize: 10, padding: '1px 6px', borderRadius: 3,
          color: borderColor, background: 'var(--color-border-light)',
        }}>
          {statusText}
        </span>
      </div>
      {/* output 区域 */}
      {hasOutput && (
        <div style={{ marginTop: 6 }}>
          {isLong && (
            <Button
              type="link" size="small"
              icon={expanded ? <DownOutlined /> : <RightOutlined />}
              onClick={() => setExpanded(v => !v)}
              style={{ padding: 0, fontSize: 11, height: 'auto' }}
            >
              {expanded ? '收起' : `展开完整输出（${command.output!.length} 字符）`}
            </Button>
          )}
          <pre
            data-testid={`command-output-${index}`}
            style={{
              margin: '4px 0 0',
              padding: 8,
              background: 'var(--log-bg)',
              color: 'var(--log-text)',
              borderRadius: 4,
              fontFamily: 'var(--font-mono)', fontSize: 11,
              maxHeight: expanded ? 400 : 120,
              overflow: 'auto',
              whiteSpace: 'pre-wrap',
              wordBreak: 'break-all',
            }}
          >{displayedOutput}</pre>
        </div>
      )}
      {!hasOutput && (
        <div style={{ fontSize: 11, color: 'var(--color-text-quaternary)', marginTop: 4 }}>
          （无返回结果）
        </div>
      )}
    </div>
  );
}

function formatDuration(ms: number): string {
  if (ms < 1000) return `${ms}ms`;
  if (ms < 60_000) return `${(ms / 1000).toFixed(1)}s`;
  return `${Math.floor(ms / 60_000)}m${Math.floor((ms % 60_000) / 1000)}s`;
}
