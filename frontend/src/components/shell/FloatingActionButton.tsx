/**
 * FloatingActionButton — 统一浮动操作按钮组件（桌面端 + 移动端共用）。
 *
 * 展开状态：右侧悬浮按钮组，含顶部展开触发按钮 + 闪念 + Wiki 对话。
 * 收缩状态：右侧一条极窄竖条（6px），贴边显示，点击可展开。
 */

import { useState, useCallback } from 'react';
import {
  LeftOutlined,
  RightOutlined,
  ThunderboltOutlined,
  MessageOutlined,
} from '@ant-design/icons';
import { Tooltip } from 'antd';

interface FloatingActionButtonProps {
  /** 打开闪念捕捉 Modal */
  onOpenQuickCapture: () => void;
  /** 打开 Wiki 对话（侧边/最大化模式） */
  onOpenWikiChat: () => void;
}

export function FloatingActionButton({
  onOpenQuickCapture,
  onOpenWikiChat,
}: FloatingActionButtonProps) {
  const [collapsed, setCollapsed] = useState(false);

  const handleExpand = useCallback(() => {
    setCollapsed(false);
  }, []);

  const handleCollapse = useCallback(() => {
    setCollapsed(true);
  }, []);

  const handleOpenQuickCapture = useCallback(() => {
    onOpenQuickCapture();
  }, [onOpenQuickCapture]);

  const handleOpenWikiChat = useCallback(() => {
    onOpenWikiChat();
  }, [onOpenWikiChat]);

  // ─── 收缩状态：极窄竖条 ─────────────────────────────────
  if (collapsed) {
    return (
      <Tooltip title="展开菜单" placement="left">
        <button
          className="fab-collapsed"
          onClick={handleExpand}
          aria-label="展开菜单"
        >
          <LeftOutlined style={{ fontSize: 12, color: '#fff' }} />
        </button>
      </Tooltip>
    );
  }

  // ─── 展开状态：按钮组 ─────────────────────────────────────
  return (
    <div className="fab-group">
      {/* 收缩按钮：顶部向下三角，点击后收起成窄竖条 */}
      <Tooltip title="收缩" placement="left">
        <button
          className="fab-collapse-btn"
          onClick={handleCollapse}
          aria-label="收缩"
        >
          ▾
        </button>
      </Tooltip>

      {/* 闪念按钮 */}
      <Tooltip title="闪念捕捉" placement="left">
        <button
          className="fab-item-btn fab-smart"
          onClick={handleOpenQuickCapture}
          aria-label="闪念捕捉"
        >
          <ThunderboltOutlined style={{ fontSize: 20, color: '#fff' }} />
        </button>
      </Tooltip>

      {/* Wiki 对话按钮 */}
      <Tooltip title="Wiki 对话" placement="left">
        <button
          className="fab-item-btn fab-wiki"
          onClick={handleOpenWikiChat}
          aria-label="Wiki 对话"
        >
          <MessageOutlined style={{ fontSize: 20, color: '#fff' }} />
        </button>
      </Tooltip>
    </div>
  );
}
