// Loop Studio 执行环节面板：DAG 流程图布局，支持控制流配置。
//
// 使用 dagre 自动布局，SVG 绘制有向边，支持条件分支可视化。

import { useState, useCallback } from 'react';
import {
  App as AntApp, Button, Modal, Form, Input, InputNumber, Select, Switch, Popconfirm, Empty, Space,
} from 'antd';
import { DeleteOutlined, CloseOutlined } from '@ant-design/icons';
import * as dbLoops from '@/utils/database/loops';
import * as dbSteps from '@/utils/database/steps';
import type { LoopStepDto, CreateLoopStepRequest } from '@/types/loop';
import type { StepSummary } from '@/types';
import { LoopFlowGraph } from '@/components/loop-flow/LoopFlowGraph';

interface StepsPanelProps {
  loopId: number;
  steps: LoopStepDto[];
  onChanged: () => void;
}

export function LoopStepsPanel({ loopId, steps, onChanged }: StepsPanelProps) {
  const { message } = AntApp.useApp();

  // Modal 状态
  const [modalOpen, setModalOpen] = useState(false);
  const [editingStep, setEditingStep] = useState<LoopStepDto | null>(null);
  const [saving, setSaving] = useState(false);
  const [candidates, setCandidates] = useState<StepSummary[]>([]);
  const [form] = Form.useForm<CreateLoopStepRequest & { todo_title: string }>();

  // 打开新增 Modal
  const handleOpenAdd = useCallback(async () => {
    setEditingStep(null);
    form.resetFields();
    try {
      const list = await dbSteps.listStepCandidates();
      setCandidates(list);
    } catch {
      setCandidates([]);
    }
    setModalOpen(true);
  }, [form]);

  // 点击流程图节点打开编辑
  const handleSelectStep = useCallback(async (step: LoopStepDto) => {
    setEditingStep(step);
    try {
      const list = await dbSteps.listStepCandidates();
      setCandidates(list);
    } catch {
      setCandidates([]);
    }
    form.setFieldsValue({
      name: step.name,
      todo_id: step.todo_id,
      description: step.description,
      enabled: step.enabled,
      min_rating: step.min_rating,
      unrated_policy: step.unrated_policy,
      on_success: step.on_success,
      success_goto_step_id: step.success_goto_step_id,
      on_rating_fail: step.on_rating_fail,
      fail_goto_step_id: step.fail_goto_step_id,
      skip_on_source_failed: step.skip_on_source_failed,
    });
    setModalOpen(true);
  }, [form]);

  // 保存
  const handleSave = useCallback(async () => {
    const values = await form.validateFields();
    setSaving(true);
    try {
      if (editingStep) {
        await dbLoops.updateLoopStep(loopId, editingStep.id, {
          name: values.name.trim(),
          description: values.description ?? '',
          todo_id: values.todo_id,
          run_mode: 'sequential',
          skip_on_source_failed: values.skip_on_source_failed ?? false,
          min_rating: values.min_rating ?? null,
          unrated_policy: values.unrated_policy ?? 'skip',
          enabled: values.enabled ?? true,
          on_success: values.on_success ?? 'next',
          success_goto_step_id: values.success_goto_step_id ?? null,
          on_rating_fail: values.on_rating_fail ?? 'break',
          fail_goto_step_id: values.fail_goto_step_id ?? null,
        });
        message.success('环节已更新');
      } else {
        await dbLoops.createLoopStep(loopId, {
          name: values.name.trim(),
          description: values.description ?? '',
          todo_id: values.todo_id,
          run_mode: 'sequential',
          skip_on_source_failed: values.skip_on_source_failed ?? false,
          min_rating: values.min_rating ?? null,
          unrated_policy: values.unrated_policy ?? 'skip',
          enabled: values.enabled ?? true,
          on_success: values.on_success ?? 'next',
          success_goto_step_id: values.success_goto_step_id ?? null,
          on_rating_fail: values.on_rating_fail ?? 'break',
          fail_goto_step_id: values.fail_goto_step_id ?? null,
        });
        message.success('环节已添加');
      }
      setModalOpen(false);
      onChanged();
    } catch {
      // 后端错误已有 message
    } finally {
      setSaving(false);
    }
  }, [form, loopId, editingStep, message, onChanged]);

  // 删除环节
  const handleDelete = useCallback(async (stepId: number) => {
    try {
      await dbLoops.deleteLoopStep(loopId, stepId);
      message.success('环节已删除');
      onChanged();
    } catch {
      // 后端错误已有 message
    }
  }, [loopId, message, onChanged]);

  // 选择 step 后自动填充 name
  const handleTodoChange = useCallback((todo_id: number) => {
    const selected = candidates.find(c => c.id === todo_id);
    if (selected) {
      const currentName = form.getFieldValue('name');
      if (!currentName || !editingStep) {
        form.setFieldsValue({ name: selected.title, todo_title: selected.title });
      }
    }
  }, [candidates, form, editingStep]);

  const on_success = Form.useWatch('on_success', form);
  const on_rating_fail = Form.useWatch('on_rating_fail', form);

  return (
    <>
      {/* 流程图 */}
      <LoopFlowGraph
        steps={steps}
        selectedStepId={editingStep?.id ?? null}
        onSelectStep={handleSelectStep}
        onAddStep={handleOpenAdd}
      />

      {/* 操作区：仅在选中环节时显示。提供「取消选择」回到未选中态，
          避免只能通过 modal 关闭或点空白来退出。删除用 Popconfirm 防误触。 */}
      {steps.length > 0 && editingStep && (
        <div style={{ marginTop: 8, display: 'flex', justifyContent: 'flex-end' }}>
          <Space size="small">
            <Button
              size="small"
              icon={<CloseOutlined />}
              style={{ fontSize: 12 }}
              onClick={() => setEditingStep(null)}
            >
              取消选择
            </Button>
            <Popconfirm
              title="删除环节"
              description={`确定删除「${editingStep.name}」？`}
              onConfirm={() => { handleDelete(editingStep.id); setEditingStep(null); }}
              okText="确定"
              cancelText="取消"
            >
              <Button size="small" danger icon={<DeleteOutlined />} style={{ fontSize: 12 }}>
                删除选中环节
              </Button>
            </Popconfirm>
          </Space>
        </div>
      )}

      {/* 新增 / 编辑 Modal */}
      <Modal
        title={editingStep ? '编辑环节' : '新增环节'}
        open={modalOpen}
        onCancel={() => setModalOpen(false)}
        onOk={handleSave}
        okText={editingStep ? '保存' : '添加'}
        cancelText="取消"
        confirmLoading={saving}
        width={560}
        destroyOnClose
      >
        <Form form={form} layout="vertical">
          <Form.Item label="关联环节" name="todo_id" rules={[{ required: true, message: '请选择关联的环节' }]}>
            <Select
              showSearch
              placeholder="选择已有的环节"
              optionFilterProp="label"
              onChange={handleTodoChange}
              options={candidates.map(c => ({
                label: `#${c.id} ${c.title}`,
                value: c.id,
              }))}
              notFoundContent={
                <Empty description="暂无环节，请先在环节页创建" image={Empty.PRESENTED_IMAGE_SIMPLE} />
              }
            />
          </Form.Item>
          <Form.Item label="名称" name="name" rules={[{ required: true, message: '名称必填' }]}>
            <Input maxLength={100} placeholder="环节名称" />
          </Form.Item>
          <Form.Item label="描述" name="description">
            <Input.TextArea rows={2} maxLength={500} placeholder="可选描述" />
          </Form.Item>
          <Form.Item label="启用" name="enabled" valuePropName="checked">
            <Switch />
          </Form.Item>

          {/* ── 门禁配置 ── */}
          <div style={{ fontWeight: 600, fontSize: 14, marginBottom: 12, color: 'var(--color-warning, #f97316)' }}>
            评分门禁
          </div>
          <Form.Item label="评分阈值" name="min_rating" tooltip="启用后 AI 自动评分，低于此值视为不通过（0-100，留空=不启用）">
            <InputNumber min={0} max={100} placeholder="留空=不启用" style={{ width: '100%' }} />
          </Form.Item>

          {/* ── 控制流配置 ── */}
          <div style={{ fontWeight: 600, fontSize: 14, marginTop: 16, marginBottom: 12, color: 'var(--color-primary, #0891b2)' }}>
            控制流
          </div>

          <div style={{
            // 用 success-bg 的语义变量驱动暗色背景；亮色 fallback 保持原浅绿观感
            background: 'var(--color-success-bg, #f0fdf4)',
            border: '1px solid var(--color-border-light, #f1f5f9)',
            padding: 12, borderRadius: 8, marginBottom: 12,
          }}>
            <Form.Item label="✅ 成功时" name="on_success" initialValue="next">
              <Select>
                <Select.Option value="next">下一步（顺序执行）</Select.Option>
                <Select.Option value="goto">跳转到指定环节</Select.Option>
                <Select.Option value="end">结束 Loop</Select.Option>
              </Select>
            </Form.Item>
            {on_success === 'goto' && (
              <Form.Item label="目标环节" name="success_goto_step_id">
                <Select
                  placeholder="选择目标环节"
                  options={steps
                    .filter(s => s.id !== editingStep?.id)
                    .map(s => ({ label: `${s.name} (#${s.id})`, value: s.id }))}
                />
              </Form.Item>
            )}
          </div>

          <div style={{
            // 同上，用 error-bg 让暗色下不再刺眼；保留浅红 fallback
            background: 'var(--color-error-bg, #fef2f2)',
            border: '1px solid var(--color-border-light, #f1f5f9)',
            padding: 12, borderRadius: 8, marginBottom: 12,
          }}>
            <Form.Item label="❌ 评分不通过时" name="on_rating_fail" initialValue="break">
              <Select>
                <Select.Option value="break">终止 Loop</Select.Option>
                <Select.Option value="skip">继续下一步</Select.Option>
                <Select.Option value="goto">跳转到指定环节</Select.Option>
                <Select.Option value="end">结束 Loop</Select.Option>
              </Select>
            </Form.Item>
            {on_rating_fail === 'goto' && (
              <Form.Item label="目标环节" name="fail_goto_step_id">
                <Select
                  placeholder="选择目标环节"
                  options={steps
                    .filter(s => s.id !== editingStep?.id)
                    .map(s => ({ label: `${s.name} (#${s.id})`, value: s.id }))}
                />
              </Form.Item>
            )}
          </div>

          <Form.Item label="上游失败时跳过本环节" name="skip_on_source_failed" valuePropName="checked" tooltip="当通过 goto 跳转时，如果上游环节失败，自动跳过本环节">
            <Switch />
          </Form.Item>
        </Form>
      </Modal>
    </>
  );
}
