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

  return (
    <div
      data-testid={`command-card-${index}`}
      style={{
        border: '1px solid var(--color-border-light)',
        borderLeft: `3px solid ${command.success ? 'var(--color-success)' : 'var(--color-error)'}`,
        borderRadius: 8,
        background: 'var(--color-bg-elevated)',
        overflow: 'hidden',
      }}
    >
      <CardHeader command={command} index={index} onCopy={onCopy} hasOutput={hasOutput} />
      {hasOutput ? (
        <CardOutput command={command} index={index} expanded={expanded} onToggle={() => setExpanded(v => !v)} isLong={isLong} />
      ) : (
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

/** 卡片标题行：状态 + 命令文本 + 耗时 + 复制按钮 */
function CardHeader({
  command, index, onCopy, hasOutput,
}: { command: CommandEntry; index: number; onCopy: () => void | Promise<void>; hasOutput: boolean }) {
  const statusColor = command.success ? 'var(--color-success)' : 'var(--color-error)';
  return (
    <div style={{
      display: 'flex',
      alignItems: 'center',
      gap: 8,
      padding: '8px 12px',
      // 仅在有输出时画分隔线，避免空态卡片多一条空线
      borderBottom: hasOutput ? '1px solid var(--color-border-light)' : 'none',
    }}>
      {/* 状态徽标：颜色从 token 拿，保证 light/dark 主题都正确 */}
      <span style={{
        display: 'flex', alignItems: 'center', gap: 4,
        fontSize: 12, color: statusColor, flexShrink: 0,
      }}>
        {command.success
          ? <CheckCircleFilled style={{ color: 'var(--color-success)' }} />
          : <CloseCircleFilled style={{ color: 'var(--color-error)' }} />}
        <span>{command.success ? '成功' : '失败'}</span>
      </span>
      {/* 等宽命令文本：单行省略号，避免长命令撑爆布局 */}
      <code
        data-testid={`command-text-${index}`}
        style={{
          flex: 1, minWidth: 0,
          fontFamily: 'var(--font-mono)',
          fontSize: 13, fontWeight: 500,
          color: 'var(--color-text)',
          overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap',
        }}
      >
        <span style={{ color: 'var(--color-text-tertiary)', userSelect: 'none' }}>$ </span>
        {command.command || '(空命令)'}
      </code>
      {/* 右侧操作区：耗时 chip + 复制按钮 */}
      <div style={{ display: 'flex', alignItems: 'center', gap: 4, flexShrink: 0 }}>
        {command.durationMs != null && (
          <span style={{
            display: 'inline-flex', alignItems: 'center', gap: 3,
            fontSize: 11, color: 'var(--color-text-tertiary)',
            padding: '2px 6px', borderRadius: 4, background: 'var(--color-bg)',
          }}>
            <ClockCircleOutlined />
            {formatDuration(command.durationMs)}
          </span>
        )}
        <Tooltip title="复制命令">
          <Button
            type="text" size="small"
            icon={<CopyOutlined />}
            onClick={onCopy}
            aria-label="复制命令"
            data-testid={`command-copy-${index}`}
            style={{ color: 'var(--color-text-tertiary)' }}
          />
        </Tooltip>
      </div>
    </div>
  );
}

/** 卡片输出区：长输出默认折叠（展示前 N 字符 + 省略号），点击展开/收起 */
function CardOutput({
  command, index, expanded, onToggle, isLong,
}: { command: CommandEntry; index: number; expanded: boolean; onToggle: () => void; isLong: boolean }) {
  // 折叠态截断到 OUTPUT_PREVIEW_LIMIT；展开态直接展示原文
  const displayedOutput = !expanded && isLong
    ? `${command.output!.slice(0, OUTPUT_PREVIEW_LIMIT)}…`
    : command.output || '';
  return (
    <div>
      {isLong && (
        <button
          type="button"
          onClick={onToggle}
          aria-expanded={expanded}
          aria-controls={`command-output-${index}`}
          className="command-toggle-btn"
          style={{
            display: 'flex', alignItems: 'center', gap: 4,
            width: '100%',
            padding: '5px 12px',
            border: 'none',
            // 背景走 .command-toggle-btn 的 CSS 变量，
            // :hover 才能真正覆盖；inline background 会把 hover 锁死
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
      {/* maxHeight 120 = 折叠预览（~10 行）；展开后 400 给出读全文的滚动条 */}
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
  );
}

/** 格式化毫秒为人类可读时长 */
function formatDuration(ms: number): string {
  if (ms < 1000) return `${ms}ms`;
  if (ms < 60_000) return `${(ms / 1000).toFixed(1)}s`;
  return `${Math.floor(ms / 60_000)}m${Math.floor((ms % 60_000) / 1000)}s`;
}
