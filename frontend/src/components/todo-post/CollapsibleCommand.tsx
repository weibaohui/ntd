// 可折叠的命令展示组件，与 CollapsibleConclusion 样式完全一致：
// 可折叠头部（标题/字数/复制按钮） + 展开后正文渲染。

import { useState } from 'react';
import { Button, message as antdMessage } from 'antd';
import { CaretDownOutlined, CaretUpOutlined } from '@ant-design/icons';
import { CopyButton } from '@/components/CopyButton';

interface CollapsibleCommandProps {
  command: string;
  /** 标题文字，Drawer 内用"命令"，PostCard 中用"命令(验证-Drawer外)"便于区分 */
  title?: string;
  messageApi?: any;
}

/**
 * 可折叠的命令展示，与 CollapsibleConclusion 样式完全一致：
 * 可折叠头部（标题/字数/复制按钮） + 展开后正文渲染。
 * 用于验证命令复制按钮在 Drawer 内/外的表现差异。
 */
export function CollapsibleCommand({ command, title = "命令", messageApi: msgApi }: CollapsibleCommandProps) {
  // 默认折叠：命令通常是长文本，默认收起减少页面高度；用户有需要再手动展开
  const [collapsed, setCollapsed] = useState(true);
  const toggle = () => setCollapsed(prev => !prev);

  const handleCopy = () => {
    const api = msgApi ?? antdMessage;
    api.success('已复制到剪贴板');
  };

  const contentId = `command-content-${title}`;

  return (
    <div
      style={{ marginBottom: 12 }}
      data-testid={`collapsible-command-${title}`}
    >
      <div
        style={{
          display: 'flex',
          justifyContent: 'space-between',
          alignItems: 'center',
          marginBottom: collapsed ? 0 : 4,
          gap: 8,
        }}
      >
        <Button
          type="text"
          size="small"
          onClick={toggle}
          icon={collapsed ? <CaretDownOutlined /> : <CaretUpOutlined />}
          aria-expanded={!collapsed}
          aria-controls={contentId}
          aria-label={collapsed ? '展开命令' : '折叠命令'}
          style={{ display: 'inline-flex', alignItems: 'center', gap: 4, padding: '0 8px' }}
        >
          <span
            style={{
              fontSize: 13,
              fontWeight: 600,
              color: 'var(--color-text)',
              marginRight: 4,
            }}
          >
            {title}
          </span>
          <span
            style={{
              fontSize: 11,
              color: 'var(--color-text-tertiary)',
              fontWeight: 500,
            }}
          >
            {[...command].length} 字
          </span>
        </Button>
        <CopyButton
          text={command}
          type="text"
          size="small"
          onCopy={handleCopy}
          aria-label={`复制${title}`}
          data-testid={`command-copy-${title}`}
        />
      </div>
      {!collapsed && (
        <div
          id={contentId}
          style={{
            background: 'var(--log-bg)',
            borderRadius: 6,
            padding: '8px 12px',
            fontFamily: 'var(--font-mono)',
            fontSize: 11,
            whiteSpace: 'pre-wrap',
            wordBreak: 'break-all',
            color: 'var(--log-text)',
          }}
        >
          {command}
        </div>
      )}
    </div>
  );
}
