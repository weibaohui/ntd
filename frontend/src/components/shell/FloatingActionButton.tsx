/**
 * FloatingActionButton — 统一浮动操作按钮组件（桌面端 + 移动端共用）。
 *
 * 始终显示两个操作按钮：闪念（⚡）+ Wiki 对话（💬）。
 * 紧凑排列，右下角 fixed 定位，基本不遮挡页面。
 */

import { ThunderboltOutlined, MessageOutlined } from '@ant-design/icons';
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
  return (
    <div className="fab-group">
      {/* 闪念按钮 */}
      <Tooltip title="闪念捕捉" placement="left">
        <button
          className="fab-item-btn fab-smart"
          onClick={onOpenQuickCapture}
          aria-label="闪念捕捉"
        >
          <ThunderboltOutlined style={{ fontSize: 20, color: '#fff' }} />
        </button>
      </Tooltip>

      {/* Wiki 对话按钮 */}
      <Tooltip title="Wiki 对话" placement="left">
        <button
          className="fab-item-btn fab-wiki"
          onClick={onOpenWikiChat}
          aria-label="Wiki 对话"
        >
          <MessageOutlined style={{ fontSize: 20, color: '#fff' }} />
        </button>
      </Tooltip>
    </div>
  );
}
