/**
 * WikiChatFloatingWindow — 全局 Wiki 对话漂浮窗口。
 *
 * 在所有页面都可以唤出，支持三种布局模式：
 * - 最小化：右下角一个圆形悬浮按钮，点击展开
 * - 侧边：右侧抽屉式面板，宽度可调节
 * - 最大化：全屏模态窗口，沉浸式对话体验
 *
 * 与 BlackboardPage 解耦，通过 state.selectedWorkspace 获取当前工作空间。
 * 使用 WebSocket 流式接收执行器日志，复用执行详情页的日志样式。
 */

import { useState, useEffect, useCallback, useRef } from 'react';
import { Button, Input, Skeleton, message, Tooltip, Drawer } from 'antd';
import {
  MessageOutlined,
  MinusOutlined,
  FullscreenOutlined,
  FullscreenExitOutlined,
  CloseOutlined,
  ColumnHeightOutlined,
} from '@ant-design/icons';
import { XMarkdown } from '@ant-design/x-markdown';
import { useTheme } from '@/hooks/useTheme';
import { useApp } from '@/hooks/useApp';
import { useIsMobile } from '@/hooks/useIsMobile';
import { ExecutorPickerPopover } from '@/components/common/ExecutorPickerPopover';
import { LOG_TYPE_COLORS_LIGHT, LOG_TYPE_COLORS_DARK, LOG_TYPE_LABELS, getLastExecutor, setLastExecutor } from '@/constants';
import { chatWithWiki } from '@/utils/database/blackboard';
import type { LogEntry } from '@/types';

const { TextArea } = Input;

/** 对话消息：支持用户提问、执行器日志（流式）、最终结果三种类型 */
type ChatMessage =
  | {
      id: string;
      role: 'user';
      content: string;
    }
  | {
      id: string;
      role: 'log';
      entry: LogEntry;
      taskId: string;
    }
  | {
      id: string;
      role: 'result';
      content: string;
      taskId: string;
      success: boolean;
      durationSecs?: number;
    };

/** 窗口布局模式 */
export type WikiChatMode = 'minimized' | 'side' | 'maximized';

interface WikiChatFloatingWindowProps {
  /** 默认布局模式 */
  defaultMode?: WikiChatMode;
}

/** 侧边模式下默认宽度（px） */
const SIDE_MODE_WIDTH = 400;
/** 最小化模式下悬浮按钮大小（px）—— 与 QuickCaptureButton 保持一致 */
const MINIMIZED_SIZE = 48;
/** 悬浮按钮距离右下角的间距（px）—— 与 QuickCaptureButton 保持一致 */
const FLOATING_MARGIN = 24;
/** 悬浮按钮之间的垂直间距（px）—— 与闪念按钮错开排列 */
const FLOATING_BUTTON_GAP = 16;
/** Wiki 对话按钮距离底部的偏移量（闪念按钮在最下方，Wiki 在它上方） */
const WIKI_BUTTON_BOTTOM_OFFSET = FLOATING_MARGIN + MINIMIZED_SIZE + FLOATING_BUTTON_GAP;

/**
 * 全局 Wiki 对话漂浮窗口组件。
 *
 * 通过 localStorage 记住用户偏好的布局模式，下次打开自动恢复。
 */
export function WikiChatFloatingWindow({ defaultMode = 'minimized' }: WikiChatFloatingWindowProps) {
  const { state } = useApp();
  const { themeMode } = useTheme();
  const isDark = themeMode === 'dark';
  const isMobile = useIsMobile();

  // ─── 布局模式状态 ────────────────────────────────────────────

  const [mode, setMode] = useState<WikiChatMode>(() => {
    try {
      const saved = localStorage.getItem('wiki_chat_mode') as WikiChatMode | null;
      if (saved && ['minimized', 'side', 'maximized'].includes(saved)) {
        return saved;
      }
    } catch {
      // 读取失败使用默认值
    }
    return defaultMode;
  });

  const [sideWidth, setSideWidth] = useState<number>(() => {
    try {
      const saved = localStorage.getItem('wiki_chat_side_width');
      if (saved) {
        const num = parseInt(saved, 10);
        if (!Number.isNaN(num) && num >= 300 && num <= 800) return num;
      }
    } catch {
      // 读取失败使用默认值
    }
    return SIDE_MODE_WIDTH;
  });

  // 持久化模式偏好
  useEffect(() => {
    try {
      localStorage.setItem('wiki_chat_mode', mode);
    } catch {
      // 忽略存储失败
    }
  }, [mode]);

  // 持久化侧边宽度
  useEffect(() => {
    try {
      localStorage.setItem('wiki_chat_side_width', String(sideWidth));
    } catch {
      // 忽略存储失败
    }
  }, [sideWidth]);

  // ─── 对话状态 ───────────────────────────────────────────────

  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [inputValue, setInputValue] = useState('');
  const [loading, setLoading] = useState(false);
  // 默认选中最后一次使用的执行器，与闪念创建界面逻辑一致
  const [chatExecutor, setChatExecutor] = useState<string>(getLastExecutor);
  const listRef = useRef<HTMLDivElement>(null);
  const currentChatTaskIdRef = useRef<string | null>(null);
  const workspaceId = state.selectedWorkspace;

  // 选择执行器时记住最后一次使用的执行器
  const handleExecutorChange = useCallback((value: string) => {
    setChatExecutor(value);
    setLastExecutor(value);
  }, []);

  // workspace 切换时清空对话历史（不同 workspace 的 wiki 内容完全隔离）
  useEffect(() => {
    setMessages([]);
    setInputValue('');
    setLoading(false);
    currentChatTaskIdRef.current = null;
  }, [workspaceId]);

  // ─── WebSocket 事件监听 ─────────────────────────────────────

  useEffect(() => {
    if (workspaceId == null) return;

    const handleStarted = (e: Event) => {
      const detail = (e as CustomEvent).detail;
      if (detail.workspace_id !== workspaceId) return;
      currentChatTaskIdRef.current = detail.task_id;
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

  // ─── 自动滚动到底部 ─────────────────────────────────────────

  useEffect(() => {
    if (listRef.current) {
      listRef.current.scrollTop = listRef.current.scrollHeight;
    }
  }, [messages, loading]);

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

    try {
      const resp = await chatWithWiki(workspaceId, text, chatExecutor);
      // WS 兜底：如果 WS 已经处理完了就不再重复加
      if (currentChatTaskIdRef.current === resp.task_id) {
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
      }
    } catch (err) {
      message.error('对话失败: ' + (err instanceof Error ? err.message : String(err)));
      setLoading(false);
      currentChatTaskIdRef.current = null;
    }
  }, [inputValue, loading, workspaceId, chatExecutor]);

  // ─── 工具函数 ───────────────────────────────────────────────

  const formatTime = (timestamp?: string) => {
    if (!timestamp) return '';
    try {
      const d = new Date(timestamp);
      if (Number.isNaN(d.getTime())) return '';
      const pad = (n: number) => String(n).padStart(2, '0');
      return `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
    } catch {
      return '';
    }
  };

  const logTypeColors = isDark ? LOG_TYPE_COLORS_DARK : LOG_TYPE_COLORS_LIGHT;

  // ─── 面板颜色变量 ───────────────────────────────────────────

  const panelBg = isDark ? '#1a1a1a' : '#ffffff';
  const panelBorder = isDark ? '#333' : '#e8e8e8';
  const userMsgBg = isDark ? '#1d3950' : '#e6f4ff';
  const textColor = isDark ? '#e0e0e0' : '#333';
  const hintColor = isDark ? '#666' : '#999';
  const headerBg = isDark ? '#222' : '#fafafa';

  // ─── 模式切换按钮 ──────────────────────────────────────────

  const ModeToggleButton = () => (
    <div style={{ display: 'flex', gap: 4 }}>
      {mode !== 'side' && (
        <Tooltip title="侧边模式">
          <Button
            type="text"
            size="small"
            icon={<ColumnHeightOutlined />}
            onClick={() => setMode('side')}
            style={{ color: hintColor }}
          />
        </Tooltip>
      )}
      {mode !== 'maximized' && (
        <Tooltip title="最大化">
          <Button
            type="text"
            size="small"
            icon={<FullscreenOutlined />}
            onClick={() => setMode('maximized')}
            style={{ color: hintColor }}
          />
        </Tooltip>
      )}
      {mode === 'maximized' && (
        <Tooltip title="还原">
          <Button
            type="text"
            size="small"
            icon={<FullscreenExitOutlined />}
            onClick={() => setMode('side')}
            style={{ color: hintColor }}
          />
        </Tooltip>
      )}
      {mode !== 'minimized' && (
        <Tooltip title="最小化">
          <Button
            type="text"
            size="small"
            icon={<MinusOutlined />}
            onClick={() => setMode('minimized')}
            style={{ color: hintColor }}
          />
        </Tooltip>
      )}
    </div>
  );

  // ─── 消息列表（公共渲染） ───────────────────────────────────

  const renderMessageList = (mobile = false) => (
    <div
      ref={listRef}
      style={{
        flex: 1,
        overflowY: 'auto',
        padding: mobile ? '12px 14px' : '16px',
        display: 'flex',
        flexDirection: 'column',
        gap: mobile ? 14 : 12,
        minHeight: 0,
      }}
    >
      {messages.length === 0 && !loading && (
        <div style={{ textAlign: 'center', color: hintColor, fontSize: mobile ? 14 : 13, padding: '32px 0' }}>
          <MessageOutlined style={{ fontSize: 36, marginBottom: 12, opacity: 0.3 }} />
          <div>还没有对话记录</div>
          <div style={{ marginTop: 6, fontSize: mobile ? 13 : 12 }}>输入问题开始与 Wiki 交互</div>
        </div>
      )}
      {messages.map((msg) => {
        if (msg.role === 'user') {
          return (
            <div key={msg.id} style={{ display: 'flex', justifyContent: 'flex-end' }}>
              <div
                style={{
                  maxWidth: mobile ? '85%' : '80%',
                  padding: mobile ? '12px 16px' : '10px 14px',
                  borderRadius: 12,
                  background: userMsgBg,
                  color: textColor,
                  fontSize: mobile ? 16 : 14,
                  lineHeight: 1.6,
                  whiteSpace: 'pre-wrap',
                  wordBreak: 'break-word',
                }}
              >
                {msg.content}
              </div>
            </div>
          );
        }
        if (msg.role === 'log') {
          const typeColor = logTypeColors[msg.entry.type] || logTypeColors.info;
          const typeLabel = LOG_TYPE_LABELS[msg.entry.type] || msg.entry.type;
          return (
            <div key={msg.id} style={{ display: 'flex', justifyContent: 'flex-start' }}>
              <div
                style={{
                  maxWidth: '100%',
                  fontSize: mobile ? 13 : 12,
                  lineHeight: 1.6,
                  fontFamily: 'ui-monospace, SFMono-Regular, Menlo, monospace',
                  color: textColor,
                  wordBreak: 'break-word',
                }}
              >
                <div style={{ display: 'flex', alignItems: 'center', gap: 6, marginBottom: 2 }}>
                  {msg.entry.timestamp && (
                    <span style={{ color: hintColor, fontSize: mobile ? 12 : 11 }}>
                      {formatTime(msg.entry.timestamp)}
                    </span>
                  )}
                  <span
                    style={{
                      padding: '1px 6px',
                      borderRadius: 3,
                      fontSize: mobile ? 11 : 10,
                      fontWeight: 600,
                      background: typeColor,
                      color: '#fff',
                      textTransform: 'uppercase',
                    }}
                  >
                    {typeLabel}
                  </span>
                </div>
                <div style={{ whiteSpace: 'pre-wrap', paddingLeft: 4 }}>
                  {msg.entry.content}
                </div>
              </div>
            </div>
          );
        }
        // result
        return (
          <div key={msg.id} style={{ display: 'flex', justifyContent: 'flex-start' }}>
            <div
              style={{
                maxWidth: mobile ? '92%' : '90%',
                padding: mobile ? '14px 16px' : '12px 14px',
                borderRadius: 12,
                background: isDark ? '#2a2a2a' : '#fff',
                color: textColor,
                fontSize: mobile ? 15 : 14,
                lineHeight: 1.6,
                border: `2px solid ${msg.success
                  ? isDark ? '#3d7a3d' : '#52c41a'
                  : isDark ? '#7a3d3d' : '#ff4d4f'}`,
                wordBreak: 'break-word',
              }}
            >
              <XMarkdown>{msg.content}</XMarkdown>
              <div style={{ marginTop: 8, fontSize: mobile ? 12 : 11, color: hintColor, display: 'flex', justifyContent: 'space-between' }}>
                <span>{msg.success ? '✅ 执行成功' : '❌ 执行失败'}</span>
                {msg.durationSecs != null && <span>用时 {msg.durationSecs.toFixed(1)}s</span>}
              </div>
            </div>
          </div>
        );
      })}
      {loading && (
        <div style={{ display: 'flex', justifyContent: 'flex-start' }}>
          <div
            style={{
              padding: mobile ? '12px 16px' : '10px 14px',
              borderRadius: 12,
              background: isDark ? '#2a2a2a' : '#fff',
              border: `1px solid ${panelBorder}`,
            }}
          >
            <Skeleton.Input active size={mobile ? 'default' : 'small'} style={{ width: 140 }} />
          </div>
        </div>
      )}
    </div>
  );

  // ─── 输入框（公共渲染） ────────────────────────────────────

  const renderInput = (mobile = false) => (
    <div
      style={{
        padding: mobile ? '12px 14px' : '12px',
        borderTop: `1px solid ${panelBorder}`,
        flexShrink: 0,
        background: panelBg,
        // 移动端适配底部安全区域，避免键盘弹出时输入框被遮挡
        paddingBottom: mobile
          ? 'calc(12px + env(safe-area-inset-bottom, 0px))'
          : '12px',
      }}
    >
      <TextArea
        value={inputValue}
        onChange={(e) => setInputValue(e.target.value)}
        placeholder="向 Wiki 提问..."
        autoSize={{ minRows: 1, maxRows: mobile ? 4 : 6 }}
        disabled={loading || workspaceId == null}
        onKeyDown={(e) => {
          if (e.key === 'Enter' && !e.shiftKey) {
            e.preventDefault();
            handleSend();
          }
        }}
        style={{ fontSize: mobile ? 16 : 14 }}
      />
      {/* 执行器选择行：与闪念创建界面保持一致的控件和逻辑 */}
      <div style={{ marginTop: 10, display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
          <span style={{ fontSize: mobile ? 13 : 12, color: hintColor }}>执行器</span>
          <ExecutorPickerPopover
            value={chatExecutor}
            onChange={handleExecutorChange}
          />
        </div>
        {!mobile && (
          <span style={{ fontSize: 11, color: hintColor }}>
            Enter 发送 · Shift+Enter 换行
            {workspaceId == null && ' · 请先选择工作空间'}
          </span>
        )}
      </div>
      <div style={{ marginTop: mobile ? 10 : 8, display: 'flex', justifyContent: 'flex-end' }}>
        <Button
          type="primary"
          size={mobile ? 'middle' : 'small'}
          onClick={handleSend}
          loading={loading}
          disabled={workspaceId == null}
          style={{ minWidth: mobile ? 80 : 'auto' }}
        >
          发送
        </Button>
      </div>
    </div>
  );

  // ─── 最小化模式：悬浮按钮 ──────────────────────────────────

  if (mode === 'minimized') {
    // 移动端：悬浮按钮位置上移，避开底部安全区域，且尺寸略大方便触控
    const bottomOffset = isMobile
      ? 'calc(env(safe-area-inset-bottom, 0px) + 80px)'
      : WIKI_BUTTON_BOTTOM_OFFSET;
    return (
      <Tooltip title="Wiki 对话" placement="left">
        <button
          onClick={() => setMode(isMobile ? 'maximized' : 'side')}
          style={{
            position: 'fixed',
            bottom: bottomOffset,
            right: FLOATING_MARGIN,
            width: isMobile ? 52 : MINIMIZED_SIZE,
            height: isMobile ? 52 : MINIMIZED_SIZE,
            borderRadius: '50%',
            background: 'var(--color-primary)',
            color: '#fff',
            border: 'none',
            cursor: 'pointer',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            boxShadow: '0 4px 16px rgba(0,0,0,0.2)',
            transition: 'transform 0.2s, box-shadow 0.2s',
            zIndex: 1000,
          }}
          onMouseEnter={(e) => {
            e.currentTarget.style.transform = 'scale(1.1)';
            e.currentTarget.style.boxShadow = '0 6px 20px rgba(0,0,0,0.3)';
          }}
          onMouseLeave={(e) => {
            e.currentTarget.style.transform = 'scale(1)';
            e.currentTarget.style.boxShadow = '0 4px 16px rgba(0,0,0,0.2)';
          }}
          aria-label="Wiki 对话"
        >
          <MessageOutlined style={{ fontSize: isMobile ? 24 : 22 }} />
        </button>
      </Tooltip>
    );
  }

  // ─── 移动端：底部 Drawer ──────────────────────────────────

  if (isMobile) {
    return (
      <Drawer
        title="Wiki 对话"
        placement="bottom"
        open={true}
        onClose={() => setMode('minimized')}
        height="85vh"
        destroyOnClose
        styles={{
          body: {
            padding: 0,
            display: 'flex',
            flexDirection: 'column',
            height: 'calc(85vh - 55px)',
            background: panelBg,
          },
          header: {
            background: headerBg,
            borderBottom: `1px solid ${panelBorder}`,
            padding: '12px 16px',
          },
        }}
        extra={
          <Button
            type="text"
            size="small"
            icon={<CloseOutlined />}
            onClick={() => setMode('minimized')}
            style={{ color: hintColor }}
          />
        }
      >
        {renderMessageList(true)}
        {renderInput(true)}
      </Drawer>
    );
  }

  // ─── 侧边模式：右侧抽屉 ────────────────────────────────────

  if (mode === 'side') {
    return (
      <div
        style={{
          position: 'fixed',
          top: 0,
          right: 0,
          width: sideWidth,
          height: '100vh',
          background: panelBg,
          borderLeft: `1px solid ${panelBorder}`,
          display: 'flex',
          flexDirection: 'column',
          zIndex: 999,
          boxShadow: '-4px 0 20px rgba(0,0,0,0.08)',
        }}
      >
        {/* 头部 */}
        <div
          style={{
            padding: '12px 16px',
            borderBottom: `1px solid ${panelBorder}`,
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'space-between',
            flexShrink: 0,
            background: headerBg,
          }}
        >
          <span style={{ fontWeight: 600, fontSize: 15, color: textColor }}>
            <MessageOutlined style={{ marginRight: 8 }} />
            Wiki 对话
          </span>
          <ModeToggleButton />
        </div>
        {/* 消息列表 */}
        {renderMessageList()}
        {/* 输入框 */}
        {renderInput()}
        {/* 拖拽调整宽度的手柄 */}
        <div
          style={{
            position: 'absolute',
            left: 0,
            top: 0,
            bottom: 0,
            width: 4,
            cursor: 'ew-resize',
            zIndex: 1,
          }}
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
            };
            document.addEventListener('mousemove', handleMouseMove);
            document.addEventListener('mouseup', handleMouseUp);
          }}
        />
      </div>
    );
  }

  // ─── 最大化模式：全屏模态 ──────────────────────────────────

  return (
    <div
      style={{
        position: 'fixed',
        inset: 0,
        background: isDark ? 'rgba(0,0,0,0.85)' : 'rgba(0,0,0,0.5)',
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        zIndex: 2000,
      }}
      onClick={(e) => {
        // 点击遮罩关闭（回到侧边模式，而不是最小化，更符合用户预期）
        if (e.target === e.currentTarget) {
          setMode('side');
        }
      }}
    >
      <div
        style={{
          width: '90vw',
          maxWidth: 900,
          height: '85vh',
          background: panelBg,
          borderRadius: 12,
          display: 'flex',
          flexDirection: 'column',
          boxShadow: '0 20px 60px rgba(0,0,0,0.3)',
          overflow: 'hidden',
        }}
      >
        {/* 头部 */}
        <div
          style={{
            padding: '14px 20px',
            borderBottom: `1px solid ${panelBorder}`,
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'space-between',
            flexShrink: 0,
            background: headerBg,
          }}
        >
          <span style={{ fontWeight: 600, fontSize: 16, color: textColor }}>
            <MessageOutlined style={{ marginRight: 10 }} />
            Wiki 对话
          </span>
          <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
            <ModeToggleButton />
            <Tooltip title="关闭">
              <Button
                type="text"
                size="small"
                icon={<CloseOutlined />}
                onClick={() => setMode('side')}
                style={{ color: hintColor }}
              />
            </Tooltip>
          </div>
        </div>
        {/* 消息列表 */}
        {renderMessageList()}
        {/* 输入框 */}
        {renderInput()}
      </div>
    </div>
  );
}
