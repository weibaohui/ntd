// 移动端顶部导航栏组件。

import { ArrowLeftOutlined, MenuOutlined } from '@ant-design/icons';
import type { View } from '@/hooks/useViewState';

interface MobileHeaderProps {
  activeView: View;
  activePanel: string;
  onBackToList: () => void;
  onOpenNav: () => void;
}

export function MobileHeader({ activeView, activePanel, onBackToList, onOpenNav }: MobileHeaderProps) {
  // todos/loops/tasks 详情页显示返回按钮，否则空占位保持菜单按钮位置一致
  // 028：activePanel 由 useViewState 派生（todoDetailId/loopDetailId/taskDetailId != null 时为 'detail'）
  // 062：补上 tasks——此前任务详情在移动端无返回入口，与事项/环路不一致。
  const showBackButton = activeView === 'todos' || activeView === 'loops' || activeView === 'tasks';

  return (
    <div className="mobile-header">
      {showBackButton && activePanel === 'detail' ? (
        <button
          className="mobile-header-menu-btn"
          onClick={onBackToList}
          aria-label="返回列表"
        >
          <ArrowLeftOutlined />
        </button>
      ) : (
        <div style={{ width: 40 }} />
      )}
      <button
        className="mobile-header-menu-btn"
        onClick={onOpenNav}
        aria-label="打开菜单"
      >
        <MenuOutlined />
      </button>
    </div>
  );
}
