import { useState, useEffect, useCallback, useRef } from 'react';
import { Modal, Drawer, Button, App } from 'antd';
import { ThunderboltOutlined, ClockCircleOutlined } from '@ant-design/icons';
import { WorkspaceSelect } from '@/components/common/WorkspaceSelect';
import { ExecutorPickerPopover } from '@/components/common/ExecutorPickerPopover';
import * as db from '@/utils/database';
import { DEFAULT_EXECUTOR } from '@/types';

interface QuickCaptureModalProps {
  open: boolean;
  onClose: () => void;
  isMobile: boolean;
  defaultWorkspaceId?: number | null;
  onCreated?: (todoId: number) => void;
  onExecuted?: (taskId: string, recordId: number) => void;
}

// 从内容中截取简洁标题（最多 30 字符）
function extractTitle(content: string): string {
  const firstLine = content.split('\n')[0]?.trim() ?? '';
  if (firstLine.length <= 30) return firstLine;
  return firstLine.slice(0, 30) + '...';
}

// localStorage key for remembering last executor choice
const STORAGE_KEY_LAST_EXECUTOR = 'ntd_quick_capture_last_executor';

function getLastExecutor(): string {
  try {
    return localStorage.getItem(STORAGE_KEY_LAST_EXECUTOR) || DEFAULT_EXECUTOR;
  } catch {
    return DEFAULT_EXECUTOR;
  }
}

function setLastExecutor(executor: string) {
  try {
    localStorage.setItem(STORAGE_KEY_LAST_EXECUTOR, executor);
  } catch {}
}

export function QuickCaptureModal({
  open,
  onClose,
  isMobile,
  defaultWorkspaceId,
  onCreated,
  onExecuted,
}: QuickCaptureModalProps) {
  const { message } = App.useApp();
  const [content, setContent] = useState('');
  const [workspaceId, setWorkspaceId] = useState<number | null>(defaultWorkspaceId ?? null);
  const [executor, setExecutor] = useState<string>(getLastExecutor);
  const [loading, setLoading] = useState(false);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  // 打开时自动聚焦
  useEffect(() => {
    if (open && !isMobile) {
      setTimeout(() => textareaRef.current?.focus(), 100);
    }
  }, [open, isMobile]);

  // 打开时重置工作空间为默认值
  useEffect(() => {
    if (open && defaultWorkspaceId != null) {
      setWorkspaceId(defaultWorkspaceId);
    }
  }, [open, defaultWorkspaceId]);

  // 关闭时清空
  const handleClose = useCallback(() => {
    setContent('');
    setLoading(false);
    onClose();
  }, [onClose]);

  // 选择执行器时记住
  const handleExecutorChange = useCallback((value: string) => {
    setExecutor(value);
    setLastExecutor(value);
  }, []);

  // 创建任务（稍后执行）
  const handleCreateLater = useCallback(async () => {
    const trimmed = content.trim();
    if (!trimmed) {
      message.warning('请输入内容');
      return;
    }
    if (!workspaceId) {
      message.warning('请选择工作空间');
      return;
    }

    setLoading(true);
    try {
      const title = extractTitle(trimmed);
      const todo = await db.createTodo(
        title,
        trimmed,
        [],
        workspaceId,
      );

      // 更新执行器
      if (executor !== DEFAULT_EXECUTOR) {
        await db.updateTodo(
          todo.id,
          title,
          trimmed,
          'pending',
          executor,
        );
      }

      message.success('已创建任务');
      onCreated?.(todo.id);
      handleClose();
    } catch (err: any) {
      message.error(err?.message || '创建失败');
    } finally {
      setLoading(false);
    }
  }, [content, workspaceId, executor, message, onCreated, handleClose]);

  // 立即执行
  const handleExecuteNow = useCallback(async () => {
    const trimmed = content.trim();
    if (!trimmed) {
      message.warning('请输入内容');
      return;
    }
    if (!workspaceId) {
      message.warning('请选择工作空间');
      return;
    }

    setLoading(true);
    try {
      const title = extractTitle(trimmed);
      const todo = await db.createTodo(
        title,
        trimmed,
        [],
        workspaceId,
      );

      // 更新执行器
      if (executor !== DEFAULT_EXECUTOR) {
        await db.updateTodo(
          todo.id,
          title,
          trimmed,
          'pending',
          executor,
        );
      }

      // 立即执行
      const result = await db.executeTodo(todo.id, executor);
      message.success('已创建并开始执行');
      onExecuted?.(result.task_id, result.record_id);
      handleClose();
    } catch (err: any) {
      message.error(err?.message || '执行失败');
    } finally {
      setLoading(false);
    }
  }, [content, workspaceId, executor, message, onExecuted, handleClose]);

  // Ctrl+Enter 快捷提交
  const handleKeyDown = useCallback((e: React.KeyboardEvent) => {
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
      e.preventDefault();
      handleExecuteNow();
    }
  }, [handleExecuteNow]);

  const bodyContent = (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
      {/* 内容输入框 */}
      <textarea
        ref={textareaRef}
        value={content}
        onChange={(e) => setContent(e.target.value)}
        onKeyDown={handleKeyDown}
        placeholder="要做什么..."
        rows={4}
        disabled={loading}
        style={{
          width: '100%',
          padding: '12px',
          borderRadius: 8,
          border: '1px solid var(--color-border-secondary)',
          background: 'var(--color-bg-elevated)',
          color: 'var(--color-text)',
          fontSize: 14,
          resize: 'vertical',
          outline: 'none',
          fontFamily: 'inherit',
        }}
      />

      {/* 工作空间选择 */}
      <div>
        <div style={{ marginBottom: 6, fontWeight: 600, fontSize: 13, color: 'var(--color-text-secondary)' }}>
          工作空间
        </div>
        <WorkspaceSelect
          value={workspaceId}
          onChange={(v) => setWorkspaceId(v)}
          required
        />
      </div>

      {/* 执行器选择（Popover） */}
      <div>
        <div style={{ marginBottom: 6, fontWeight: 600, fontSize: 13, color: 'var(--color-text-secondary)' }}>
          执行器
        </div>
        <ExecutorPickerPopover
          value={executor}
          onChange={handleExecutorChange}
        />
      </div>

      {/* 操作按钮 */}
      <div style={{ display: 'flex', gap: 8, marginTop: 8 }}>
        <Button
          onClick={handleCreateLater}
          loading={loading}
          disabled={!content.trim() || !workspaceId}
          style={{ flex: 1 }}
        >
          <ClockCircleOutlined style={{ marginRight: 4 }} />
          稍后
        </Button>
        <Button
          type="primary"
          icon={<ThunderboltOutlined />}
          onClick={handleExecuteNow}
          loading={loading}
          disabled={!content.trim() || !workspaceId}
          style={{ flex: 1 }}
        >
          立即执行
        </Button>
      </div>

      {/* 快捷键提示 */}
      {!isMobile && (
        <div style={{ textAlign: 'center', fontSize: 12, color: 'var(--color-text-tertiary)' }}>
          <div>Ctrl+Enter / ⌘+Enter 立即执行</div>
          <div style={{ marginTop: 4 }}>Ctrl+K / ⌘+K 随时唤出</div>
        </div>
      )}
    </div>
  );

  // 移动端：底部 Drawer
  if (isMobile) {
    return (
      <Drawer
        title="闪念捕捉"
        placement="bottom"
        open={open}
        onClose={handleClose}
        height="auto"
        destroyOnClose
      >
        {bodyContent}
      </Drawer>
    );
  }

  // 桌面端：居中 Modal
  return (
    <Modal
      title="闪念捕捉"
      open={open}
      onCancel={handleClose}
      width={480}
      centered
      destroyOnClose
      footer={null}
    >
      {bodyContent}
    </Modal>
  );
}
