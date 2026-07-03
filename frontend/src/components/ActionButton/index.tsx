import { useState, useEffect, useCallback } from 'react';
import { Button, Drawer, Spin, Typography, Space, message, Input, Tag } from 'antd';
import { ThunderboltOutlined, EditOutlined } from '@ant-design/icons';
import { useIsMobile } from '@/hooks/useIsMobile';
import { useActionExecution } from './useActionExecution';
import { ExecutorPicker } from '@/components/todo-drawer/ExecutorPicker';
import { EXECUTORS_FOR_PICKER } from '@/types/execution';
import { getLastExecutor, setLastExecutor } from '@/constants';
import type { ActionButtonProps } from './types';

const { Text, Paragraph } = Typography;
const { TextArea } = Input;

/**
 * 可复用的一键 AI 执行组件。
 *
 * 交互流程：
 * 1. 点击按钮 → 打开 Drawer
 * 2. 展示可编辑的 Prompt、执行器选择器、参数预览
 * 3. 用户可修改后点击「执行」
 * 4. 通过 WebSocket 监听执行完成
 * 5. 完成后展示完整 markdown 结果
 * 6. 用户选择「应用」或「拒绝」
 */
export function ActionButton({
  actionType,
  actionKey,
  prompt,
  params,
  onApply,
  workspaceId,
  children,
  buttonType = 'default',
  icon,
  disabled = false,
  panelTitle = '自动优化标题',
  panelDescription = '检查并确认以下内容后执行',
  executor,
}: ActionButtonProps) {
  const [open, setOpen] = useState(false);
  const [editablePrompt, setEditablePrompt] = useState(prompt);
  // 初始化 selectedExecutor：优先从 localStorage 恢复上次选择，不存在时回退到 prop executor
  const [selectedExecutor, setSelectedExecutor] = useState<string | undefined>(
    () => getLastExecutor(executor)
  );
  const isMobile = useIsMobile();
  const { status, result, error, execute, retry, reset } = useActionExecution(
    actionType,
    actionKey,
    prompt,
    params,
    workspaceId,
    executor,
  );

  // 打开 Drawer 时：
  // - 重置 editablePrompt 为最新的 prompt 默认值
  // - 从 localStorage 恢复上次选的执行器（覆盖 prop 传入的默认值）
  //   这样用户每次打开都是自己上次的选择，而不是每次回到默认。
  useEffect(() => {
    if (open) {
      setEditablePrompt(prompt);
      const saved = getLastExecutor(executor);
      setSelectedExecutor(saved);
    }
  }, [open, prompt, executor, actionType, actionKey]);

  // 用户切换执行器时同时保存选择到 localStorage，
  // 确保本次关闭后下次打开 Drawer 能恢复成这个值。
  const handleExecutorChange = useCallback((value: string) => {
    setSelectedExecutor(value);
    setLastExecutor(value);
  }, []);

  const handleOpen = () => {
    reset();
    setOpen(true);
  };

  const handleClose = () => {
    setOpen(false);
  };

  const handleExecute = () => {
    execute(editablePrompt, selectedExecutor);
  };

  const handleRetry = () => {
    retry(editablePrompt, selectedExecutor);
  };

  const handleApply = async () => {
    if (!result) return;
    try {
      await onApply(result);
      message.success('已应用');
      handleClose();
    } catch (err: any) {
      message.error(err?.message || '应用失败');
    }
  };

  // 从 params 中提取要展示的预览内容
  const paramsPreview = Object.entries(params)
    .map(([key, value]) => ({ key, value }));

  const renderContent = () => {
    if (status === 'idle') {
      return (
        <Space direction="vertical" size="middle" style={{ width: '100%' }}>
          {/* 描述 */}
          <Text type="secondary">{panelDescription}</Text>

          {/* Prompt 编辑区 */}
          <div>
            <div style={{ display: 'flex', alignItems: 'center', gap: 6, marginBottom: 6 }}>
              <EditOutlined style={{ color: 'var(--color-text-secondary)' }} />
              <Text strong style={{ fontSize: 13 }}>Prompt 模板</Text>
            </div>
            <TextArea
              value={editablePrompt}
              onChange={(e) => setEditablePrompt(e.target.value)}
              autoSize={{ minRows: 4, maxRows: 12 }}
              style={{ fontFamily: 'monospace', fontSize: 12 }}
            />
          </div>

          {/* 执行器选择（选择后自动记住到 localStorage，下次打开同理恢复） */}
          <ExecutorPicker
            executor={selectedExecutor || 'claudecode'}
            executorOptions={EXECUTORS_FOR_PICKER}
            onChange={handleExecutorChange}
          />

          {/* 参数预览 */}
          {paramsPreview.length > 0 && (
            <div>
              <Text strong style={{ fontSize: 13, display: 'block', marginBottom: 6 }}>
                模板参数
              </Text>
              <div
                style={{
                  padding: 10,
                  background: 'var(--color-bg-elevated)',
                  border: '1px solid var(--color-border-secondary)',
                  borderRadius: 6,
                  maxHeight: 150,
                  overflow: 'auto',
                }}
              >
                {paramsPreview.map(({ key, value }) => (
                  <div key={key} style={{ marginBottom: 8 }}>
                    <Tag color="blue" style={{ marginBottom: 4 }}>{`{{${key}}}`}</Tag>
                    <div style={{
                      fontSize: 12,
                      whiteSpace: 'pre-wrap',
                      wordBreak: 'break-word',
                      color: 'var(--color-text-secondary)',
                    }}>
                      {value}
                    </div>
                  </div>
                ))}
              </div>
            </div>
          )}
        </Space>
      );
    }

    if (status === 'executing') {
      return (
        <div style={{ textAlign: 'center', padding: '40px 0' }}>
          <Spin size="large" />
          <div style={{ marginTop: 16 }}>
            <Text type="secondary">AI 正在处理中...</Text>
          </div>
        </div>
      );
    }

    if (status === 'failed') {
      return (
        <Space direction="vertical" size="middle" style={{ width: '100%' }}>
          <Text type="danger">{error || '执行失败'}</Text>
        </Space>
      );
    }

    // completed
    return (
      <Space direction="vertical" size="middle" style={{ width: '100%' }}>
        <Text type="secondary">AI 生成结果：</Text>
        <div
          style={{
            padding: 12,
            background: 'var(--color-success-bg, #f6ffed)',
            border: '1px solid var(--color-success-border, #b7eb8f)',
            borderRadius: 6,
            maxHeight: 400,
            overflow: 'auto',
          }}
        >
          <Paragraph
            style={{ whiteSpace: 'pre-wrap', margin: 0 }}
            ellipsis={{ expandable: true, symbol: '展开' }}
          >
            {result}
          </Paragraph>
        </div>
      </Space>
    );
  };

  const renderFooter = () => {
    if (status === 'idle') {
      return (
        <Space>
          <Button onClick={handleClose}>取消</Button>
          <Button type="primary" onClick={handleExecute}>
            执行
          </Button>
        </Space>
      );
    }

    if (status === 'executing') {
      return null;
    }

    if (status === 'failed') {
      return (
        <Space>
          <Button onClick={handleClose}>关闭</Button>
          <Button type="primary" onClick={handleRetry}>
            重试
          </Button>
        </Space>
      );
    }

    // completed
    return (
      <Space>
        <Button onClick={handleClose}>拒绝</Button>
        <Button type="primary" onClick={handleApply}>
          应用
        </Button>
      </Space>
    );
  };

  return (
    <>
      <Button
        type={buttonType}
        icon={icon || <ThunderboltOutlined />}
        onClick={handleOpen}
        disabled={disabled}
      >
        {children || '优化标题'}
      </Button>

      <Drawer
        title={panelTitle}
        open={open}
        onClose={status !== 'executing' ? handleClose : undefined} // 执行中禁止关闭，其他时候允许通过 X 按钮关闭
        closable={status !== 'executing'}
        keyboard={false} // 禁止 Escape 关闭
        maskClosable={false} // 始终禁止点击遮罩关闭
        placement={isMobile ? 'bottom' : 'right'}
        width={isMobile ? '100%' : 520}
        height={isMobile ? '85vh' : undefined}
        footer={renderFooter()}
        destroyOnHidden
      >
        {renderContent()}
      </Drawer>
    </>
  );
}
