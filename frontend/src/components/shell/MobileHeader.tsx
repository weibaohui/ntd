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
  // items/loops 详情页显示返回按钮，否则空占位保持菜单按钮位置一致
  const showBackButton = activeView === 'items' || activeView === 'loops';

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
