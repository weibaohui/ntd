import { useState, useRef, useEffect } from 'react';
import { Modal, Drawer, Input, Button, message } from 'antd';
import { ThunderboltOutlined, BulbOutlined, SettingOutlined, WarningOutlined } from '@ant-design/icons';
import { smartCreate } from '../utils/database';
import type { Config } from '../types';

interface SmartCreateModalProps {
  open: boolean;
  onClose: () => void;
  isMobile: boolean;
  config: Config | null;
  onGoToSettings?: () => void;
  onSubmitted?: () => void;
}

export function SmartCreateModal({ open, onClose, isMobile, config, onGoToSettings, onSubmitted }: SmartCreateModalProps) {
  const [content, setContent] = useState('');
  const [submitting, setSubmitting] = useState(false);
  const textAreaRef = useRef<any>(null);
  const defaultTodoId = config?.default_response_todo_id ?? null;

  // 打开时自动聚焦
  useEffect(() => {
    if (open && !isMobile) {
      setTimeout(() => textAreaRef.current?.focus(), 100);
    }
  }, [open, isMobile]);

  // 关闭时清空
  const handleClose = () => {
    setContent('');
    setSubmitting(false);
    onClose();
  };

  const handleSubmit = async () => {
    const trimmed = content.trim();
    if (!trimmed) return;

    setSubmitting(true);
    try {
      const result = await smartCreate(trimmed);
      message.success(`已提交智能执行，任务 #${result.todo_id} ${result.todo_title} 正在处理中`);
      onSubmitted?.();
      handleClose();
    } catch (err: any) {
      message.error(err?.message || '智能执行失败');
    } finally {
      setSubmitting(false);
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
      e.preventDefault();
      handleSubmit();
    }
  };

  // 默认响应未配置
  const renderEmptyState = () => (
    <div className="smart-create-empty">
      <WarningOutlined className="smart-create-empty-icon" />
      <div className="smart-create-empty-title">尚未配置默认响应 Todo</div>
      <div className="smart-create-empty-desc">
        智能新建需要先在「设置 → 消息规则 → 默认响应」中指定一个 Todo 作为默认处理器。
      </div>
      <Button
        type="primary"
        icon={<SettingOutlined />}
        onClick={() => {
          handleClose();
          onGoToSettings?.();
        }}
      >
        前往配置
      </Button>
    </div>
  );

  // 正常内容
  const renderContent = () => (
    <div className="smart-create-content">
      <Input.TextArea
        ref={textAreaRef}
        value={content}
        onChange={(e) => setContent(e.target.value)}
        onKeyDown={handleKeyDown}
        placeholder="描述你的需求，AI 会自动处理..."
        autoSize={{ minRows: 3, maxRows: 8 }}
        disabled={submitting}
      />
      <div className="smart-create-hint">
        <BulbOutlined style={{ marginRight: 4 }} />
        输入自然语言描述，如"帮我分析一下最近的销售数据趋势"或"写一封周报总结"
      </div>
      {defaultTodoId && (
        <div className="smart-create-config-info">
          <SettingOutlined />
          默认响应：#{defaultTodoId}
        </div>
      )}
      <div className="smart-create-footer">
        {!isMobile && (
          <Button onClick={handleClose} disabled={submitting}>
            取消
          </Button>
        )}
        <Button
          type="primary"
          icon={<ThunderboltOutlined />}
          onClick={handleSubmit}
          loading={submitting}
          disabled={!content.trim()}
          block={isMobile}
        >
          {submitting ? '执行中...' : '智能执行'}
        </Button>
        {!isMobile && !submitting && content.trim() && (
          <span className="smart-create-shortcut">Ctrl+Enter</span>
        )}
      </div>
    </div>
  );

  const bodyContent = defaultTodoId ? renderContent() : renderEmptyState();

  // 移动端：底部 Drawer
  if (isMobile) {
    return (
      <Drawer
        title={<><ThunderboltOutlined style={{ marginRight: 8 }} />智能新建</>}
        placement="bottom"
        open={open}
        onClose={handleClose}
        height="auto"
        className="smart-create-drawer"
        destroyOnClose
      >
        {bodyContent}
      </Drawer>
    );
  }

  // PC 端：居中 Modal
  return (
    <Modal
      title={<><ThunderboltOutlined style={{ marginRight: 8 }} />智能新建</>}
      open={open}
      onCancel={handleClose}
      width={520}
      centered
      destroyOnClose
      footer={null}
    >
      {bodyContent}
    </Modal>
  );
}
