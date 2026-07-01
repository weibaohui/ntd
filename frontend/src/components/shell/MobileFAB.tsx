// 移动端 FAB 浮动按钮组组件。

import { PlusOutlined, CloseOutlined, ThunderboltOutlined } from '@ant-design/icons';

interface MobileFABProps {
  expanded: boolean;
  onToggle: () => void;
  onOpenQuickCapture: () => void;
  onOpenCreate: () => void;
}

export function MobileFAB({ expanded, onToggle, onOpenQuickCapture, onOpenCreate }: MobileFABProps) {
  return (
    <>
      {expanded && (
        <div className="mobile-fab-backdrop" onClick={() => onToggle()} />
      )}
      <div className="mobile-fab-group">
        {expanded && (
          <>
            <div className="mobile-fab-item" style={{ animationDelay: '0ms' }}>
              <span className="mobile-fab-item-label">闪念</span>
              <button
                className="mobile-fab-item-btn mobile-fab-smart"
                onClick={() => { onToggle(); onOpenQuickCapture(); }}
                aria-label="闪念捕捉"
              >
                <ThunderboltOutlined style={{ fontSize: 20, color: '#fff' }} />
              </button>
            </div>
            <div className="mobile-fab-item" style={{ animationDelay: '50ms' }}>
              <span className="mobile-fab-item-label">新建</span>
              <button
                className="mobile-fab-item-btn mobile-fab-create"
                onClick={() => { onToggle(); onOpenCreate(); }}
                aria-label="新建任务"
              >
                <PlusOutlined style={{ fontSize: 20, color: '#fff' }} />
              </button>
            </div>
          </>
        )}
        <button
          className={`mobile-fab-main ${expanded ? 'expanded' : ''}`}
          onClick={onToggle}
          aria-label={expanded ? '关闭' : '创建任务'}
        >
          {expanded
            ? <CloseOutlined style={{ fontSize: 22, color: '#fff' }} />
            : <PlusOutlined style={{ fontSize: 24, color: '#fff' }} />}
        </button>
      </div>
    </>
  );
}
