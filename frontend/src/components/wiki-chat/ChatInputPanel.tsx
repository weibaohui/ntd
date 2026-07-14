/**
 * ChatInputPanel — Wiki 对话输入面板组件。
 *
 * 包含输入框、执行器选择、专家选择、工作空间切换、发送按钮。
 */

import { Button, Input } from 'antd';
import { ExecutorPickerPopover } from '@/components/common/ExecutorPickerPopover';
// 导入专家选择器组件，用于选择专家/专家团来注入专家上下文
import { ExpertPicker } from '@/components/todo-drawer/ExpertPicker';
import { WorkspaceSwitcher } from '@/components/shell/WorkspaceSwitcher';
import { getChatColors } from './ChatMessageItem';

const { TextArea } = Input;

interface ChatInputPanelProps {
  /** 输入框内容 */
  inputValue: string;
  /** 输入框内容变化回调 */
  onInputChange: (value: string) => void;
  /** 发送按钮点击回调 */
  onSend: () => void;
  /** 是否正在加载 */
  loading: boolean;
  /** 当前选中的工作空间 ID */
  workspaceId: number | null;
  /** 当前选中的执行器名称 */
  chatExecutor: string;
  /** 执行器变化回调 */
  onExecutorChange: (value: string) => void;
  /** 当前选中的专家名称，null/undefined 表示未选择 */
  expertName?: string | null;
  /** 专家变化回调 */
  onExpertChange: (expertName: string | null) => void;
  /** 工作空间切换回调 */
  onWorkspaceChange: (id: number | null) => void;
  /** 是否移动端布局 */
  mobile?: boolean;
  /** 是否暗色主题 */
  isDark: boolean;
}

/** 输入面板组件：TextArea + 执行器选择 + 专家选择 + 发送按钮 */
export function ChatInputPanel({
  inputValue,
  onInputChange,
  onSend,
  loading,
  workspaceId,
  chatExecutor,
  onExecutorChange,
  expertName,
  onExpertChange,
  onWorkspaceChange,
  mobile = false,
  isDark,
}: ChatInputPanelProps) {
  const colors = getChatColors(isDark);

  // Enter 发送，Shift+Enter 换行
  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      onSend();
    }
  };

  return (
    <div
      style={{
        padding: mobile ? '12px 14px' : '12px',
        borderTop: `1px solid ${colors.panelBorder}`,
        flexShrink: 0,
        background: colors.panelBg,
        // 移动端适配底部安全区域
        paddingBottom: mobile
          ? 'calc(12px + env(safe-area-inset-bottom, 0px))'
          : '12px',
      }}
    >
      <TextArea
        value={inputValue}
        onChange={(e) => onInputChange(e.target.value)}
        placeholder="向 Wiki 提问..."
        autoSize={{ minRows: 1, maxRows: mobile ? 4 : 6 }}
        disabled={loading || workspaceId == null}
        onKeyDown={handleKeyDown}
        style={{ fontSize: mobile ? 16 : 14 }}
      />

      {/* 快捷键提示：输入框下方单独一行小字 */}
      {!mobile && (
        <div style={{ textAlign: 'center', fontSize: 11, color: colors.hintColor, marginTop: 6 }}>
          Enter 发送 · Shift+Enter 换行
          {workspaceId == null && ' · 请先选择工作空间'}
        </div>
      )}

      {/* 工作空间 + 执行器 横排布局：左工作空间、右执行器 */}
      <div style={{ marginTop: mobile ? 8 : 10, display: 'flex', gap: 16 }}>
        {/* 工作空间 */}
        <div style={{ flex: 1 }}>
          <div style={{ fontSize: 12, color: colors.hintColor, marginBottom: 6 }}>
            工作空间
          </div>
          <WorkspaceSwitcher
            value={workspaceId ?? null}
            showAddOption={false}
            onChange={onWorkspaceChange}
          />
        </div>
        {/* 执行器 */}
        <div style={{ flex: 1 }}>
          <div style={{ fontSize: 12, color: colors.hintColor, marginBottom: 6 }}>
            执行器
          </div>
          <ExecutorPickerPopover
            value={chatExecutor}
            onChange={onExecutorChange}
          />
        </div>
      </div>

      {/* 专家选择器 */}
      <div style={{ marginTop: 12 }}>
        <div style={{ fontSize: 12, color: colors.hintColor, marginBottom: 6 }}>
          专家/团队
        </div>
        <ExpertPicker
          value={expertName}
          onChange={onExpertChange}
        />
      </div>

      {/* 发送按钮 */}
      <div style={{ marginTop: mobile ? 10 : 8, display: 'flex', justifyContent: 'flex-end' }}>
        <Button
          type="primary"
          size={mobile ? 'middle' : 'small'}
          onClick={onSend}
          loading={loading}
          disabled={workspaceId == null}
          style={{ minWidth: mobile ? 80 : 'auto' }}
        >
          发送
        </Button>
      </div>
    </div>
  );
}
