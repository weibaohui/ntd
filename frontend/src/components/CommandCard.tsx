/**
 * issue #648: 单条命令卡片 — 终端风格
 *
 * 设计要点：
 * - 左侧 3px 状态色边框标识成功/失败
 * - 顶部：状态图标 + 命令文本（等宽） + 复制按钮 + 耗时
 * - 输出区：终端风格暗底（暗色主题）或浅灰底（亮色主题），等宽字体
 * - 长输出默认折叠，点击展开/收起
 * - 所有交互元素有 cursor:pointer 和 hover 反馈
 */
import { useState } from 'react';
import { Button, Tooltip, message } from 'antd';
import {
  CopyOutlined,
  CheckCircleFilled,
  CloseCircleFilled,
  ClockCircleOutlined,
  DownOutlined,
  RightOutlined,
} from '@ant-design/icons';
import type { CommandEntry } from '@/types';
import { copyToClipboard } from '@/utils/clipboard';

/** 输出预览截断阈值 */
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

  // 状态色：成功用绿色，失败用红色
  const statusColor = command.success ? 'var(--color-success)' : 'var(--color-error)';
  const statusText = command.success ? '成功' : '失败';
  const StatusIcon = command.success
    ? <CheckCircleFilled style={{ color: 'var(--color-success)' }} />
    : <CloseCircleFilled style={{ color: 'var(--color-error)' }} />;

  // 折叠状态下只展示前 N 字符 + 省略号
  const displayedOutput = hasOutput && !expanded
    ? `${command.output!.slice(0, OUTPUT_PREVIEW_LIMIT)}…`
    : command.output || '';

  return (
    <div
      data-testid={`command-card-${index}`}
      style={{
        border: '1px solid var(--color-border-light)',
        borderLeft: `3px solid ${statusColor}`,
        borderRadius: 8,
        background: 'var(--color-bg-elevated)',
        overflow: 'hidden',
      }}
    >
      {/* 标题行：状态 + 命令 + 操作 */}
      <div style={{
        display: 'flex',
        alignItems: 'center',
        gap: 8,
        padding: '8px 12px',
        borderBottom: hasOutput ? '1px solid var(--color-border-light)' : 'none',
      }}>
        {/* 状态图标 + 文本 */}
        <span style={{
          display: 'flex',
          alignItems: 'center',
          gap: 4,
          fontSize: 12,
          color: statusColor,
          flexShrink: 0,
        }}>
          {StatusIcon}
          <span>{statusText}</span>
        </span>
        {/* 命令文本 */}
        <code
          data-testid={`command-text-${index}`}
          style={{
            flex: 1,
            minWidth: 0,
            fontFamily: 'var(--font-mono)',
            fontSize: 13,
            fontWeight: 500,
            color: 'var(--color-text)',
            overflow: 'hidden',
            textOverflow: 'ellipsis',
            whiteSpace: 'nowrap',
          }}
        >
          <span style={{ color: 'var(--color-text-tertiary)', userSelect: 'none' }}>$ </span>
          {command.command || '(空命令)'}
        </code>
        {/* 右侧操作区 */}
        <div style={{ display: 'flex', alignItems: 'center', gap: 4, flexShrink: 0 }}>
          {/* 耗时 */}
          {command.durationMs != null && (
            <span style={{
              display: 'inline-flex',
              alignItems: 'center',
              gap: 3,
              fontSize: 11,
              color: 'var(--color-text-tertiary)',
              padding: '2px 6px',
              borderRadius: 4,
              background: 'var(--color-bg)',
            }}>
              <ClockCircleOutlined />
              {formatDuration(command.durationMs)}
            </span>
          )}
          {/* 复制按钮 */}
          <Tooltip title="复制命令">
            <Button
              type="text"
              size="small"
              icon={<CopyOutlined />}
              onClick={onCopy}
              aria-label="复制命令"
              data-testid={`command-copy-${index}`}
              style={{ color: 'var(--color-text-tertiary)' }}
            />
          </Tooltip>
        </div>
      </div>

      {/* 输出区域 */}
      {hasOutput && (
        <div>
          {/* 展开/收起按钮 */}
          {isLong && (
            <button
              type="button"
              onClick={() => setExpanded(v => !v)}
              aria-expanded={expanded}
              aria-controls={`command-output-${index}`}
              className="command-toggle-btn"
              style={{
                display: 'flex',
                alignItems: 'center',
                gap: 4,
                width: '100%',
                padding: '5px 12px',
                border: 'none',
                background: 'var(--color-bg)',
                color: 'var(--color-primary)',
                fontSize: 11,
                fontFamily: 'var(--font-sans)',
                cursor: 'pointer',
              }}
            >
              {expanded ? <DownOutlined /> : <RightOutlined />}
              {expanded ? '收起输出' : `展开全部（${command.output!.length} 字符）`}
            </button>
          )}
          {/* 输出内容 */}
          <pre
            id={`command-output-${index}`}
            data-testid={`command-output-${index}`}
            style={{
              margin: 0,
              padding: '10px 12px',
              background: 'var(--command-output-bg, var(--color-bg))',
              color: 'var(--color-text-secondary)',
              fontFamily: 'var(--font-mono)',
              fontSize: 12,
              lineHeight: 1.6,
              maxHeight: expanded ? 400 : 120,
              overflow: 'auto',
              whiteSpace: 'pre-wrap',
              wordBreak: 'break-all',
            }}
          >{displayedOutput}</pre>
        </div>
      )}

      {/* 无输出提示 */}
      {!hasOutput && (
        <div style={{
          padding: '6px 12px',
          fontSize: 12,
          color: 'var(--color-text-tertiary)',
          fontStyle: 'italic',
        }}>
          无返回结果
        </div>
      )}
    </div>
  );
}

/** 格式化毫秒为人类可读时长 */
function formatDuration(ms: number): string {
  if (ms < 1000) return `${ms}ms`;
  if (ms < 60_000) return `${(ms / 1000).toFixed(1)}s`;
  return `${Math.floor(ms / 60_000)}m${Math.floor((ms % 60_000) / 1000)}s`;
}
