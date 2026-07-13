/**
 * WikiChatFloatingWindow — 全局 Wiki 对话漂浮窗口主组件。
 *
 * 负责状态管理、WebSocket 事件监听、布局模式切换。
 * 子组件：ChatMessageList、ChatInputPanel、ModeToggleButtons。
 */

import { useState, useEffect, useCallback, useRef } from 'react';
import { Button, Tooltip, Drawer, Modal, message } from 'antd';
import { MessageOutlined, CloseOutlined } from '@ant-design/icons';
import { useTheme } from '@/hooks/useTheme';
import { useApp } from '@/hooks/useApp';
import { useIsMobile } from '@/hooks/useIsMobile';
import { getLastExecutor, setLastExecutor } from '@/constants';
import { chatWithWiki } from '@/utils/database/blackboard';
import { ChatMessageList } from './wiki-chat/ChatMessageList';
import { ChatInputPanel } from './wiki-chat/ChatInputPanel';
import { ModeToggleButtons } from './wiki-chat/ModeToggleButtons';
import { ChatMessage, getChatColors } from './wiki-chat/ChatMessageItem';
import type { WikiChatMode } from './wiki-chat/types';

// 导出类型供外部使用（如 FloatingActionButton）
export type { WikiChatMode } from './wiki-chat/types';

/** 侧边模式下默认宽度（px） */
const SIDE_MODE_WIDTH = 400;

interface WikiChatFloatingWindowProps {
  /** 默认布局模式 */
  defaultMode?: WikiChatMode;
  /** 强制指定布局模式 */
  forceMode?: WikiChatMode;
  /** 关闭回调 */
  onClose?: () => void;
}

/** 全局 Wiki 对话漂浮窗口主组件 */
export function WikiChatFloatingWindow({ defaultMode = 'minimized', forceMode, onClose }: WikiChatFloatingWindowProps) {
  const { state, dispatch } = useApp();
  const { themeMode } = useTheme();
  const isDark = themeMode === 'dark';
  const isMobile = useIsMobile();
  const colors = getChatColors(isDark);

  // ─── 布局模式状态 ────────────────────────────────────────────
  const [mode, setMode] = useState<WikiChatMode>(() => {
    if (forceMode !== undefined) return forceMode;
    try {
      const saved = localStorage.getItem('wiki_chat_mode') as WikiChatMode | null;
      if (saved && ['minimized', 'side', 'maximized'].includes(saved)) return saved;
    } catch {}
    return defaultMode;
  });

  // forceMode 变化时同步更新内部 mode
  useEffect(() => {
    if (forceMode !== undefined) setMode(forceMode);
  }, [forceMode]);

  const [sideWidth, setSideWidth] = useState<number>(() => {
    try {
      const saved = localStorage.getItem('wiki_chat_side_width');
      if (saved) {
        const num = parseInt(saved, 10);
        if (!Number.isNaN(num) && num >= 300 && num <= 800) return num;
      }
    } catch {}
    return SIDE_MODE_WIDTH;
  });

  // 持久化模式偏好
  useEffect(() => {
    if (forceMode !== undefined) return;
    try { localStorage.setItem('wiki_chat_mode', mode); } catch {}
  }, [mode, forceMode]);

  // 持久化侧边宽度
  useEffect(() => {
    try { localStorage.setItem('wiki_chat_side_width', String(sideWidth)); } catch {}
  }, [sideWidth]);

  // 卸载时清理可能残留的拖拽监听器
  useEffect(() => {
    return () => {
      if (resizeCleanupRef.current) {
        resizeCleanupRef.current();
        resizeCleanupRef.current = null;
      }
    };
  }, []);

  // ─── 对话状态 ───────────────────────────────────────────────
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [inputValue, setInputValue] = useState('');
  const [loading, setLoading] = useState(false);
  const [chatExecutor, setChatExecutor] = useState<string>(getLastExecutor);
  const currentChatTaskIdRef = useRef<string | null>(null);
  const wsHandledRef = useRef<boolean>(false);
  // 用于拖拽调整宽度时，存储清理函数以便组件卸载时移除监听器
  const resizeCleanupRef = useRef<(() => void) | null>(null);
  const workspaceId = state.selectedWorkspace;

  // 选择执行器时记住最后一次使用的执行器
  const handleExecutorChange = useCallback((value: string) => {
    setChatExecutor(value);
    setLastExecutor(value);
  }, []);

  // workspace 切换时清空对话历史
  useEffect(() => {
    setMessages([]);
    setInputValue('');
    setLoading(false);
    currentChatTaskIdRef.current = null;
    wsHandledRef.current = false;
  }, [workspaceId]);

  // ─── WebSocket 事件监听 ─────────────────────────────────────
  useEffect(() => {
    if (workspaceId == null) return;

    const handleStarted = (e: Event) => {
      const detail = (e as CustomEvent).detail;
      if (detail.workspace_id !== workspaceId) return;
      currentChatTaskIdRef.current = detail.task_id;
      wsHandledRef.current = false;
    };

    const handleOutput = (e: Event) => {
      const detail = (e as CustomEvent).detail;
      if (detail.workspace_id !== workspaceId) return;
      if (!currentChatTaskIdRef.current || currentChatTaskIdRef.current !== detail.task_id) return;
      const logMsg: ChatMessage = {
        id: `log-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
        role: 'log',
        entry: detail.entry,
        taskId: detail.task_id,
      };
      setMessages(prev => [...prev, logMsg]);
    };

    const handleFinished = (e: Event) => {
      const detail = (e as CustomEvent).detail;
      if (detail.workspace_id !== workspaceId) return;
      if (!currentChatTaskIdRef.current || currentChatTaskIdRef.current !== detail.task_id) return;
      wsHandledRef.current = true;
      if (detail.result) {
        const resultMsg: ChatMessage = {
          id: `res-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
          role: 'result',
          content: detail.result,
          taskId: detail.task_id,
          success: detail.success,
          durationSecs: detail.duration_secs,
        };
        setMessages(prev => [...prev, resultMsg]);
      }
      currentChatTaskIdRef.current = null;
      setLoading(false);
    };

    window.addEventListener('wikiChatStarted', handleStarted);
    window.addEventListener('wikiChatOutput', handleOutput);
    window.addEventListener('wikiChatFinished', handleFinished);
    return () => {
      window.removeEventListener('wikiChatStarted', handleStarted);
      window.removeEventListener('wikiChatOutput', handleOutput);
      window.removeEventListener('wikiChatFinished', handleFinished);
    };
  }, [workspaceId]);

  // ─── 发送消息 ───────────────────────────────────────────────
  const handleSend = useCallback(async () => {
    const text = inputValue.trim();
    if (!text || loading || workspaceId == null) return;

    const userMsg: ChatMessage = {
      id: `u-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
      role: 'user',
      content: text,
    };
    setMessages(prev => [...prev, userMsg]);
    setInputValue('');
    setLoading(true);
    wsHandledRef.current = false;

    try {
      const resp = await chatWithWiki(workspaceId, text, chatExecutor);
      if (wsHandledRef.current) {
        currentChatTaskIdRef.current = null;
        return;
      }
      if (resp.content) {
        const resultMsg: ChatMessage = {
          id: `res-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
          role: 'result',
          content: resp.content,
          taskId: resp.task_id,
          success: resp.success,
          durationSecs: resp.duration_secs,
        };
        setMessages(prev => [...prev, resultMsg]);
      }
      currentChatTaskIdRef.current = null;
      setLoading(false);
    } catch (err) {
      message.error('对话失败: ' + (err instanceof Error ? err.message : String(err)));
      setLoading(false);
      currentChatTaskIdRef.current = null;
      wsHandledRef.current = false;
    }
  }, [inputValue, loading, workspaceId, chatExecutor]);

  // ─── 工作空间切换回调 ───────────────────────────────────────
  const handleWorkspaceChange = useCallback((id: number | null) => {
    if (id != null) {
      dispatch({ type: 'SELECT_WORKSPACE', payload: id });
    }
  }, [dispatch]);

  // ─── 移动端：底部 Drawer ──────────────────────────────────
  // 使用 forceMode 判断显隐（TypeScript 无法 narrow 来自 props 的值，避免 mode !== 'minimized' 编译错误）
  if (isMobile) {
    return (
      <Drawer
        title="Wiki 对话"
        placement="bottom"
        open={forceMode !== 'minimized'}
        onClose={() => {
          setMode('minimized');
          onClose?.();
        }}
        height="85vh"
        destroyOnHidden
        styles={{
          body: { padding: 0, display: 'flex', flexDirection: 'column', height: 'calc(85vh - 55px)', background: colors.panelBg },
          header: { background: colors.headerBg, borderBottom: `1px solid ${colors.panelBorder}`, padding: '12px 16px' },
        }}
      >
        <ChatMessageList messages={messages} loading={loading} mobile isDark={isDark} />
        <ChatInputPanel
          inputValue={inputValue}
          onInputChange={setInputValue}
          onSend={handleSend}
          loading={loading}
          workspaceId={workspaceId}
          chatExecutor={chatExecutor}
          onExecutorChange={handleExecutorChange}
          onWorkspaceChange={handleWorkspaceChange}
          mobile
          isDark={isDark}
        />
      </Drawer>
    );
  }

  // ─── 侧边模式：右侧抽屉 ────────────────────────────────────
  if (mode === 'side') {
    return (
      <div style={{ position: 'fixed', top: 0, right: 0, width: sideWidth, height: '100vh', background: colors.panelBg, borderLeft: `1px solid ${colors.panelBorder}`, display: 'flex', flexDirection: 'column', zIndex: 999, boxShadow: '-4px 0 20px rgba(0,0,0,0.08)' }}>
        {/* 头部 */}
        <div style={{ padding: '12px 16px', borderBottom: `1px solid ${colors.panelBorder}`, display: 'flex', alignItems: 'center', justifyContent: 'space-between', flexShrink: 0, background: colors.headerBg }}>
          <span style={{ fontWeight: 600, fontSize: 15, color: colors.textColor }}>
            <MessageOutlined style={{ marginRight: 8 }} />
            Wiki 对话
          </span>
          <div style={{ display: 'flex', alignItems: 'center', gap: 4 }}>
            <ModeToggleButtons mode={mode} onModeChange={setMode} onClose={onClose} isDark={isDark} />
            {onClose && (
              <Tooltip title="关闭">
                <Button type="text" size="small" icon={<CloseOutlined />} onClick={onClose} style={{ color: colors.hintColor }} />
              </Tooltip>
            )}
          </div>
        </div>
        {/* 消息列表 */}
        <ChatMessageList messages={messages} loading={loading} isDark={isDark} />
        {/* 输入框 */}
        <ChatInputPanel
          inputValue={inputValue}
          onInputChange={setInputValue}
          onSend={handleSend}
          loading={loading}
          workspaceId={workspaceId}
          chatExecutor={chatExecutor}
          onExecutorChange={handleExecutorChange}
          onWorkspaceChange={handleWorkspaceChange}
          isDark={isDark}
        />
        {/* 拖拽调整宽度的手柄 */}
        <div
          style={{ position: 'absolute', left: 0, top: 0, bottom: 0, width: 4, cursor: 'ew-resize', zIndex: 1 }}
          onMouseDown={(e) => {
            e.preventDefault();
            const startX = e.clientX;
            const startWidth = sideWidth;
            const handleMouseMove = (me: MouseEvent) => {
              const newWidth = startWidth - (me.clientX - startX);
              const clamped = Math.max(300, Math.min(800, newWidth));
              setSideWidth(clamped);
            };
            const handleMouseUp = () => {
              document.removeEventListener('mousemove', handleMouseMove);
              document.removeEventListener('mouseup', handleMouseUp);
              resizeCleanupRef.current = null;
            };
            document.addEventListener('mousemove', handleMouseMove);
            document.addEventListener('mouseup', handleMouseUp);
            resizeCleanupRef.current = handleMouseUp;
          }}
        />
      </div>
    );
  }

  // ─── 最大化模式：全屏模态 ──────────────────────────────────
  return (
    <Modal
      open={forceMode === 'maximized'}
      onCancel={() => {
        setMode('side');
        onClose?.();
      }}
      footer={null}
      width="90vw"
      styles={{
        mask: { background: isDark ? 'rgba(0,0,0,0.85)' : 'rgba(0,0,0,0.5)' },
        wrapper: { display: 'flex', alignItems: 'center', justifyContent: 'center' },
        root: { height: '85vh', background: colors.panelBg, borderRadius: 12, boxShadow: '0 20px 60px rgba(0,0,0,0.3)' },
        body: { padding: 0, display: 'flex', flexDirection: 'column', height: 'calc(85vh - 55px)', overflow: 'hidden' },
        header: { background: colors.headerBg, borderBottom: `1px solid ${colors.panelBorder}`, padding: '14px 20px' },
      }}
      title={
        <span style={{ fontWeight: 600, fontSize: 16, color: colors.textColor }}>
          <MessageOutlined style={{ marginRight: 10 }} />
          Wiki 对话
        </span>
      }
      closeIcon={<CloseOutlined style={{ color: colors.hintColor }} />}
      destroyOnClose
    >
      {/* 消息列表 */}
      <ChatMessageList messages={messages} loading={loading} isDark={isDark} />
      {/* 输入框 */}
      <ChatInputPanel
        inputValue={inputValue}
        onInputChange={setInputValue}
        onSend={handleSend}
        loading={loading}
        workspaceId={workspaceId}
        chatExecutor={chatExecutor}
        onExecutorChange={handleExecutorChange}
        onWorkspaceChange={handleWorkspaceChange}
        isDark={isDark}
      />
    </Modal>
  );
}