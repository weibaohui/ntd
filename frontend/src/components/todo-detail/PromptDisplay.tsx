import { useState } from 'react';
// 093：懒加载包装，避免 x-markdown 依赖树进入首屏静态图。
import { LazyXMarkdown } from '@/components/common/LazyXMarkdown';

/** 可展开的 Prompt 内容展示组件 */
export function PromptDisplay({ content }: { content: string }) {
  const [expanded, setExpanded] = useState(false);
  return (
    <div style={{ marginTop: 8 }}>
      <div
        onClick={() => setExpanded(!expanded)}
        style={{
          display: 'inline-flex',
          alignItems: 'center',
          gap: 4,
          fontSize: 12,
          color: 'var(--color-text-secondary)',
          cursor: 'pointer',
          userSelect: 'none',
        }}
      >
        <span>{expanded ? '▼' : '▶'}</span>
        <span>Prompt</span>
      </div>
      {expanded && (
        <div
          style={{
            marginTop: 6,
            padding: '8px 12px',
            borderRadius: 8,
            background: 'var(--color-bg-elevated)',
            border: '1px solid var(--color-border-light)',
            maxHeight: 300,
            overflow: 'auto',
          }}
        >
          <LazyXMarkdown content={content} />
        </div>
      )}
    </div>
  );
}
