// 环节详情面板 + 编辑功能。

import { useEffect, useState, useCallback } from 'react';
import {
  Skeleton, Empty, Tag, Descriptions, Button, Modal, Form, Input, Select, App as AntApp,
} from 'antd';
import { ApartmentOutlined, ThunderboltOutlined, EditOutlined } from '@ant-design/icons';
import * as dbSteps from '@/utils/database/steps';
import type { StepSummary } from '@/types';
import { formatRelativeTime } from '@/utils/datetime';

interface StepDetailPanelProps {
  stepId: number;
}

export function StepDetailPanel({ stepId }: StepDetailPanelProps) {
  const { message } = AntApp.useApp();
  const [step, setStep] = useState<StepSummary | null>(null);
  const [loading, setLoading] = useState(true);
  const [editing, setEditing] = useState(false);
  const [saving, setSaving] = useState(false);
  const [form] = Form.useForm<{ title: string; prompt: string; executor?: string; acceptance_criteria?: string }>();

  const loadStep = useCallback(() => {
    setLoading(true);
    dbSteps.getStep(stepId)
      .then(setStep)
      .catch(() => setStep(null))
      .finally(() => setLoading(false));
  }, [stepId]);

  useEffect(() => { loadStep(); }, [loadStep]);

  const handleOpenEdit = useCallback(() => {
    if (!step) return;
    form.setFieldsValue({
      title: step.title,
      prompt: step.prompt,
      executor: step.executor || undefined,
      acceptance_criteria: step.acceptance_criteria || undefined,
    });
    setEditing(true);
  }, [step, form]);

  const handleSave = useCallback(async () => {
    const values = await form.validateFields();
    setSaving(true);
    try {
      const updated = await dbSteps.updateStep(stepId, {
        title: values.title.trim(),
        prompt: values.prompt ?? '',
        executor: values.executor ?? null,
        acceptance_criteria: values.acceptance_criteria ?? null,
      });
      setStep(updated);
      message.success('环节已更新');
      setEditing(false);
    } catch {
      // ignore
    } finally {
      setSaving(false);
    }
  }, [form, stepId, message]);

  if (loading) {
    return <Skeleton active style={{ padding: 24 }} />;
  }
  if (!step) {
    return <Empty description="无法加载该环节" style={{ marginTop: 64 }} />;
  }

  return (
    <div style={{ padding: '20px 24px' }}>
      {/* Header */}
      <div style={{ display: 'flex', alignItems: 'center', gap: 12, marginBottom: 20 }}>
        <h2 style={{ margin: 0, fontSize: 18, flex: 1, minWidth: 0, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap', color: 'var(--color-text, #0f172a)' }}>
          {step.title}
        </h2>
        <span style={{ color: 'var(--color-text-tertiary, #94a3b8)', fontSize: 12, fontFamily: 'monospace' }}>#{step.id}</span>
        <Button size="small" icon={<EditOutlined />} onClick={handleOpenEdit}>编辑</Button>
      </div>

      {/* 基本信息 */}
      <section style={{
        background: 'var(--color-bg-elevated, #ffffff)',
        border: '1px solid var(--color-border, #e2e8f0)',
        borderRadius: 8,
        padding: 16,
        marginBottom: 12,
      }}>
        <div style={{ fontSize: 14, fontWeight: 600, color: 'var(--color-text, #0f172a)', marginBottom: 12 }}>基本信息</div>
        <Descriptions column={2} size="small" bordered={false}>
          <Descriptions.Item label="执行器">
            {step.executor ? (
              <span><ThunderboltOutlined style={{ color: '#fa8c16', marginRight: 4 }} />{step.executor}</span>
            ) : (
              <span style={{ color: 'var(--color-text-tertiary, #94a3b8)' }}>未指派</span>
            )}
          </Descriptions.Item>
          <Descriptions.Item label="复用次数">
            <Tag icon={<ApartmentOutlined />} color={step.used_by_loop_stage_count > 0 ? 'purple' : 'default'}>
              {step.used_by_loop_stage_count}
            </Tag>
          </Descriptions.Item>
          <Descriptions.Item label="来源事项">
            {step.source_todo_id ? (
              <span>#<code>{step.source_todo_id}</code></span>
            ) : (
              <span style={{ color: 'var(--color-text-tertiary, #94a3b8)' }}>—</span>
            )}
          </Descriptions.Item>
          <Descriptions.Item label="更新于">
            {step.updated_at ? formatRelativeTime(step.updated_at) : '—'}
          </Descriptions.Item>
        </Descriptions>
      </section>

      {/* 验收标准 */}
      {step.acceptance_criteria && (
        <section style={{
          background: 'var(--color-bg-elevated, #ffffff)',
          border: '1px solid var(--color-border, #e2e8f0)',
          borderRadius: 8,
          padding: 16,
          marginBottom: 12,
        }}>
          <div style={{ fontSize: 14, fontWeight: 600, color: 'var(--color-text, #0f172a)', marginBottom: 8 }}>验收标准</div>
          <div style={{ fontSize: 13, color: 'var(--color-text-secondary, #475569)', whiteSpace: 'pre-wrap' }}>
            {step.acceptance_criteria}
          </div>
        </section>
      )}

      {/* Prompt */}
      <section style={{
        background: 'var(--color-bg-elevated, #ffffff)',
        border: '1px solid var(--color-border, #e2e8f0)',
        borderRadius: 8,
        padding: 16,
      }}>
        <div style={{ fontSize: 14, fontWeight: 600, color: 'var(--color-text, #0f172a)', marginBottom: 8 }}>提示词 (Prompt)</div>
        <div style={{
          fontSize: 13, color: 'var(--color-text-secondary, #475569)',
          background: 'var(--color-bg-secondary, #f8fafc)',
          padding: 12, borderRadius: 6, whiteSpace: 'pre-wrap',
          lineHeight: 1.6,
        }}>
          {step.prompt || <span style={{ color: 'var(--color-text-tertiary, #94a3b8)' }}>无提示词</span>}
        </div>
      </section>

      {/* 编辑 Modal */}
      <Modal
        title="编辑环节"
        open={editing}
        onCancel={() => setEditing(false)}
        onOk={handleSave}
        okText="保存"
        cancelText="取消"
        confirmLoading={saving}
        width={560}
        destroyOnClose
      >
        <Form form={form} layout="vertical">
          <Form.Item label="名称" name="title" rules={[{ required: true, message: '名称必填' }]}>
            <Input maxLength={100} />
          </Form.Item>
          <Form.Item label="执行器" name="executor">
            <Select
              allowClear
              placeholder="选择执行器"
              options={[
                { label: 'claudecode', value: 'claudecode' },
                { label: 'codebuddy', value: 'codebuddy' },
                { label: 'opencode', value: 'opencode' },
                { label: 'atomcode', value: 'atomcode' },
                { label: 'hermes', value: 'hermes' },
                { label: 'kimi', value: 'kimi' },
                { label: 'codex', value: 'codex' },
                { label: 'codewhale', value: 'codewhale' },
                { label: 'pi', value: 'pi' },
                { label: 'mimo', value: 'mimo' },
                { label: 'zhanlu', value: 'zhanlu' },
              ]}
            />
          </Form.Item>
          <Form.Item label="提示词 (Prompt)" name="prompt" tooltip="描述这个环节能做什么">
            <Input.TextArea rows={6} maxLength={4000} placeholder="提示词内容" />
          </Form.Item>
          <Form.Item label="验收标准" name="acceptance_criteria" tooltip="判断执行结果是否满足预期的标准">
            <Input.TextArea rows={3} maxLength={2000} placeholder="验收标准（可选）" />
          </Form.Item>
        </Form>
      </Modal>
    </div>
  );
}
