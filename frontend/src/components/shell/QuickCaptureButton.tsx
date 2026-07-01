// 桌面端闪念捕捉快捷按钮组件。

import { Tooltip } from 'antd';
import { ThunderboltOutlined } from '@ant-design/icons';

interface QuickCaptureButtonProps {
  onClick: () => void;
}

export function QuickCaptureButton({ onClick }: QuickCaptureButtonProps) {
  return (
    <Tooltip title="闪念捕捉 (⌘+K)" placement="left">
      <button
        onClick={onClick}
        style={{
          position: 'fixed',
          bottom: 24,
          right: 24,
          width: 48,
          height: 48,
          borderRadius: '50%',
          background: 'var(--color-primary)',
          color: '#fff',
          border: 'none',
          cursor: 'pointer',
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center',
          boxShadow: '0 4px 12px rgba(0,0,0,0.2)',
          transition: 'transform 0.2s, box-shadow 0.2s',
          zIndex: 1000,
        }}
        onMouseEnter={(e) => {
          e.currentTarget.style.transform = 'scale(1.1)';
          e.currentTarget.style.boxShadow = '0 6px 16px rgba(0,0,0,0.3)';
        }}
        onMouseLeave={(e) => {
          e.currentTarget.style.transform = 'scale(1)';
          e.currentTarget.style.boxShadow = '0 4px 12px rgba(0,0,0,0.2)';
        }}
        aria-label="闪念捕捉"
      >
        <ThunderboltOutlined style={{ fontSize: 22 }} />
      </button>
    </Tooltip>
  );
}
