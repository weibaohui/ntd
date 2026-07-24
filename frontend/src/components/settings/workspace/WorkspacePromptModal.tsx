import { useState, useEffect } from 'react';
import { Modal, Form, Input, Alert, message } from 'antd';
import * as db from '@/utils/database';

interface WorkspacePromptModalProps {
  open: boolean;
  workspaceId: number;
  workspaceName: string;
  onClose: () => void;
  onSaved?: () => void;
}

/**
 * 工作空间基础约定弹窗：编辑和保存 system_prompt（工作空间级共识 prompt）。
 * 从 WorkspaceSettingsPanel 中提取出的独立控件，可直接在工作空间管理页打开。
 */
export function WorkspacePromptModal({ open, workspaceId, workspaceName, onClose, onSaved }: WorkspacePromptModalProps) {
  const [form] = Form.useForm();
  const [saving, setSaving] = useState(false);

  // 每次打开弹窗时加载最新的 system_prompt
  useEffect(() => {
    if (!open) return;
    db.getWorkspaceSettings(workspaceId)
      .then(s => {
        // null 视作空串让 TextArea 显示空，与 DefaultResponseConfigPanel 保持一致
        form.setFieldsValue({ system_prompt: s.system_prompt ?? '' });
      })
      .catch((err: any) => message.error('加载工作空间设置失败: ' + (err?.message || String(err))));
  }, [open, workspaceId, form]);

  const handleSave = async () => {
    try {
      const values = await form.validateFields();
      setSaving(true);
      // 保存工作空间级 prompt；空串表示显式清空
      await db.updateWorkspaceSettings(workspaceId, {
        system_prompt: values.system_prompt ?? '',
      });
      message.success('基础约定已保存');
      onSaved?.();
      onClose();
    } catch (err: any) {
      if (!err?.errorFields) {
        message.error('保存失败: ' + (err?.message || String(err)));
      }
    } finally {
      setSaving(false);
    }
  };

  return (
    <Modal
      title={`${workspaceName} - 基础约定`}
      open={open}
      onOk={handleSave}
      onCancel={onClose}
      confirmLoading={saving}
      width={640}
      destroyOnClose
    >
      <Form form={form} layout="vertical" initialValues={{ system_prompt: '' }}>
        <Form.Item
          name="system_prompt"
          label="工作空间 Prompt"
          tooltip="该工作空间下所有 todo 执行时作为前置 prompt 注入。可填写产物目录约定、认证信息、基本文件路径等共识内容。"
        >
          <Input.TextArea
            rows={8}
            maxLength={8000}
            showCount
            placeholder={
              '## 工作空间共识\n\n' +
              '- 产物目录：编译输出放在 ./target/release\n' +
              '- 认证：访问内部服务用 token xxx\n' +
              '- 项目根：/path/to/project'
            }
          />
        </Form.Item>
        <Alert
          type="warning"
          showIcon
          message="⚠️ 此处写入的内容将作为执行器前置 prompt 注入到该工作空间下所有 todo 的执行中，请谨慎填写敏感信息。"
        />
      </Form>
    </Modal>
  );
}
