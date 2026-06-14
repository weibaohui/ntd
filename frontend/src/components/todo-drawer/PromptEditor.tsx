import { memo } from 'react';
import { Button, Tooltip } from 'antd';
import { FileTextOutlined } from '@ant-design/icons';
import { PROMPT_PARAMS } from './constants';
import { MdEditor } from '@/components/MdEditor';

export const PromptEditor = memo(function PromptEditor({ value, onChange, editorRef, onOpenTemplate, onInsertText }: {
  value: string;
  onChange: (v: string) => void;
  editorRef: React.MutableRefObject<any>;
  onOpenTemplate: () => void;
  onInsertText: (text: string) => void;
}) {
  return (
    <div style={{ marginBottom: 16 }}>
      <div style={{ marginBottom: 10, fontWeight: 600, fontSize: 14, display: 'flex', alignItems: 'center', gap: 8 }}>
        <FileTextOutlined style={{ color: 'var(--color-primary)' }} />
        <span>Prompt</span>
        <Button
          size="small"
          icon={<FileTextOutlined />}
          onClick={onOpenTemplate}
          style={{ marginLeft: 'auto' }}
        >
          从模板创建
        </Button>
      </div>
      <MdEditor
        value={value}
        onChange={onChange}
        height={200}
        editorRef={editorRef}
      />
      <div style={{
        marginTop: 8,
        display: 'flex',
        flexWrap: 'wrap',
        gap: 6,
        alignItems: 'center',
      }}>
        <span style={{ fontSize: 12, color: 'var(--color-text-tertiary)', marginRight: 2 }}>可用参数:</span>
        {PROMPT_PARAMS.map(p => (
          <Tooltip key={p.key} title={p.desc}>
            <code
              onClick={() => onInsertText(p.key)}
              style={{
                fontSize: 11,
                padding: '1px 6px',
                borderRadius: 4,
                background: 'var(--color-fill-quaternary)',
                border: '1px solid var(--color-border-secondary)',
                cursor: 'pointer',
                color: 'var(--color-text-secondary)',
                transition: 'all 0.2s',
              }}
              onMouseEnter={(e) => {
                (e.currentTarget as HTMLElement).style.borderColor = 'var(--color-primary)';
                (e.currentTarget as HTMLElement).style.color = 'var(--color-primary)';
              }}
              onMouseLeave={(e) => {
                (e.currentTarget as HTMLElement).style.borderColor = 'var(--color-border-secondary)';
                (e.currentTarget as HTMLElement).style.color = 'var(--color-text-secondary)';
              }}
            >
              {p.key}
            </code>
          </Tooltip>
        ))}
      </div>
    </div>
  );
});
