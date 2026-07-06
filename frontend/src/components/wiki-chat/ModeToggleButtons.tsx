/**
 * ModeToggleButtons — Wiki 对话布局模式切换按钮组。
 *
 * 侧边模式、最大化、还原、最小化四个按钮，根据当前模式动态显示。
 */

import { Button, Tooltip } from 'antd';
import {
  ColumnHeightOutlined,
  FullscreenOutlined,
  FullscreenExitOutlined,
  MinusOutlined,
} from '@ant-design/icons';
import type { WikiChatMode } from './types';
import { getChatColors } from './ChatMessageItem';

interface ModeToggleButtonsProps {
  /** 当前布局模式 */
  mode: WikiChatMode;
  /** 模式切换回调 */
  onModeChange: (mode: WikiChatMode) => void;
  /** 关闭回调（最小化时调用） */
  onClose?: () => void;
  /** 是否暗色主题 */
  isDark: boolean;
}

/** 模式切换按钮组 */
export function ModeToggleButtons({ mode, onModeChange, onClose, isDark }: ModeToggleButtonsProps) {
  const colors = getChatColors(isDark);

  // 最小化按钮点击逻辑：优先调用 onClose，否则切换到 minimized
  const handleMinimize = () => {
    if (onClose) {
      onClose();
    } else {
      onModeChange('minimized');
    }
  };

  return (
    <div style={{ display: 'flex', gap: 4 }}>
      {mode !== 'side' && (
        <Tooltip title="侧边模式">
          <Button
            type="text"
            size="small"
            icon={<ColumnHeightOutlined />}
            onClick={() => onModeChange('side')}
            style={{ color: colors.hintColor }}
          />
        </Tooltip>
      )}
      {mode !== 'maximized' && (
        <Tooltip title="最大化">
          <Button
            type="text"
            size="small"
            icon={<FullscreenOutlined />}
            onClick={() => onModeChange('maximized')}
            style={{ color: colors.hintColor }}
          />
        </Tooltip>
      )}
      {mode === 'maximized' && (
        <Tooltip title="还原">
          <Button
            type="text"
            size="small"
            icon={<FullscreenExitOutlined />}
            onClick={() => onModeChange('side')}
            style={{ color: colors.hintColor }}
          />
        </Tooltip>
      )}
      {mode !== 'minimized' && (
        <Tooltip title="最小化">
          <Button
            type="text"
            size="small"
            icon={<MinusOutlined />}
            onClick={handleMinimize}
            style={{ color: colors.hintColor }}
          />
        </Tooltip>
      )}
    </div>
  );
}