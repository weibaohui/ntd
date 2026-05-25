import { useState } from 'react';
import XMarkdown from '@ant-design/x-markdown';

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
          <XMarkdown content={content} />
        </div>
      )}
    </div>
  );
}
