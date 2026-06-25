// 环路创建/编辑共用的 Form Modal。
//
// 设计要点：
// - mode="create"：新建环路，保存后回传新 loopId
// - mode="edit"：编辑已有环路，需传 loopId + initialData 预填
// - 工作空间为必填项（创建模式强制，编辑模式 allowClear 但保存时校验不通过)
// - 评审模板 inline 创建逻辑一并迁入，避免用户切到设置页
//
// 被 LoopStudioDetailPanel（编辑）和 App.tsx（新建）共用。

import { useEffect, useState, useCallback } from 'react';
import { App as AntApp, Drawer, Form, Input, InputNumber, Select, Button, Checkbox, Modal } from 'antd';
import { PlusOutlined } from '@ant-design/icons';
import * as dbLoops from '@/utils/database/loops';
import * as dbReviewTemplates from '@/utils/database/reviewTemplates';
import * as dbTodos from '@/utils/database/todos';
import type { UpdateLoopRequest } from '@/types/loop';
import type { ReviewTemplateOption } from '@/types/reviewTemplate';
import type { Todo } from '@/types/todo';
import { TagCheckCardGroup } from './TagCheckCard';
import { WorkspaceSelect } from './common/WorkspaceSelect';

// ---------- props ----------

interface LoopFormModalProps {
  open: boolean;
  /** 'create' = 新建, 'edit' = 编辑 */
  mode: 'create' | 'edit';
  /** 编辑模式必传，创建模式不传 */
  loopId?: number;
  /** 编辑模式预填数据（创建模式不传） */
  initialData?: {
    name: string;
    description: string;
    workspace: string | null;
    icon: string;
    review_template_id: number | null;
    tag_ids: number[];
    limits_config: string | null;
    abnormal_handler_todo_id?: number | null;
    abnormal_handler_trigger_on?: string;
  };
  /** 可用标签列表 */
  tags: Array<{ id: number; name: string; color: string }>;
  /** 保存成功回调（创建模式回传新 loopId） */
  onSaved: (loopId?: number) => void;
  onClose: () => void;
}

// ---------- Form values type ----------

type FormValues = UpdateLoopRequest & {
  max_step_executions?: number;
  max_total_tokens?: number;
  abnormal_handler_todo_id?: number | null;
  abnormal_handler_trigger_on?: string[];
};

// ---------- component ----------

export function LoopFormModal({
  open, mode, loopId, initialData, tags, onSaved, onClose,
}: LoopFormModalProps) {
  const { message } = AntApp.useApp();
  const [saving, setSaving] = useState(false);
  const [form] = Form.useForm<FormValues>();
  // 标签选中态（单选）
  const [editingTag, setEditingTag] = useState<number | null>(null);
  // 工作空间受控值（与 form.setFieldsValue 配合，避免直接操作 form 内部状态）
  const [workspaceValue, setWorkspaceValue] = useState<string | null>(null);
  // 评审模板
  const [reviewTemplateOptions, setReviewTemplateOptions] = useState<ReviewTemplateOption[]>([]);
  const [creatingTemplate, setCreatingTemplate] = useState(false);
  const [creatingTemplateSaving, setCreatingTemplateSaving] = useState(false);
  const [newTemplateForm] = Form.useForm<{ name: string; description?: string; prompt: string }>();
  // 异常处理 Todo
  const [abnormalHandlerTodoOptions, setAbnormalHandlerTodoOptions] = useState<Todo[]>([]);

  // 打开时加载评审模板选项和异常处理 Todo 选项
  useEffect(() => {
    if (!open) return;
    dbReviewTemplates.listReviewTemplateOptions()
      .then(setReviewTemplateOptions)
      .catch(() => { /* 静默 */ });
    dbTodos.getAllTodos()
      .then(setAbnormalHandlerTodoOptions)
      .catch(() => { /* 静默 */ });
  }, [open]);

  // 打开时（仅编辑模式）预填表单
  useEffect(() => {
    if (!open) return;
    if (mode === 'edit' && initialData) {
      form.setFieldsValue({
        name: initialData.name,
        description: initialData.description,
        icon: initialData.icon,
        review_template_id: initialData.review_template_id ?? null,
        abnormal_handler_todo_id: initialData.abnormal_handler_todo_id ?? null,
      });
      setWorkspaceValue(initialData.workspace ?? null);
      // 解析 limits_config
      try {
        const lc = JSON.parse(initialData.limits_config || '{}');
        form.setFieldsValue({
          max_step_executions: lc.max_step_executions ?? null,
          max_total_tokens: lc.max_total_tokens ?? null,
        });
      } catch { /* 忽略解析错误 */ }
      // 解析异常处理触发条件
      try {
        const triggerOn = JSON.parse(initialData.abnormal_handler_trigger_on || '["capped_step","capped_token","failed"]');
        form.setFieldsValue({ abnormal_handler_trigger_on: triggerOn });
      } catch {
        form.setFieldsValue({ abnormal_handler_trigger_on: ['capped_step', 'capped_token', 'failed'] });
      }
      setEditingTag(initialData.tag_ids?.[0] ?? null);
    } else if (mode === 'create') {
      // 创建模式：清空表单
      form.resetFields();
      setEditingTag(null);
      setWorkspaceValue(null);
    }
  }, [open, mode, initialData, form]);

  // 刷新评审模板并选中新建的模板
  const reloadTemplatesAndSelect = useCallback(async (selectedId: number) => {
    const opts = await dbReviewTemplates.listReviewTemplateOptions();
    setReviewTemplateOptions(opts);
    form.setFieldsValue({ review_template_id: selectedId });
  }, [form]);

  // inline 创建评审模板
  const handleCreateTemplate = useCallback(async () => {
    const values = await newTemplateForm.validateFields();
    setCreatingTemplateSaving(true);
    try {
      const created = await dbReviewTemplates.createReviewTemplate({
        name: values.name.trim(),
        description: values.description?.trim() || null,
        prompt: values.prompt,
      });
      message.success(`已创建模板「${created.name}」`);
      await reloadTemplatesAndSelect(created.id);
      newTemplateForm.resetFields();
      setCreatingTemplate(false);
    } catch (e) {
      message.error(`创建失败：${(e as Error).message}`);
    } finally {
      setCreatingTemplateSaving(false);
    }
  }, [newTemplateForm, message, reloadTemplatesAndSelect]);

  // 保存（创建 / 编辑共用）
  const handleSave = useCallback(async () => {
    const values = await form.validateFields();
    setSaving(true);
    try {
      // 构建 limits_config
      const limitsConfig: Record<string, any> = {};
      if (values.max_step_executions != null) limitsConfig.max_step_executions = values.max_step_executions;
      if (values.max_total_tokens != null) limitsConfig.max_total_tokens = values.max_total_tokens;

      // 构建异常处理触发条件
      const abnormalHandlerTriggerOn = values.abnormal_handler_trigger_on
        ? JSON.stringify(values.abnormal_handler_trigger_on)
        : '["capped_step","capped_token","failed"]';

      const basePayload = {
        name: values.name.trim(),
        description: values.description ?? '',
        workspace: workspaceValue ?? null,
        icon: values.icon ?? 'loop',
        review_template_id: values.review_template_id ?? null,
        limits_config: Object.keys(limitsConfig).length > 0 ? JSON.stringify(limitsConfig) : null,
        abnormal_handler_todo_id: values.abnormal_handler_todo_id ?? null,
        abnormal_handler_trigger_on: abnormalHandlerTriggerOn,
        tag_ids: editingTag != null ? [editingTag] : [],
      };

      if (mode === 'create') {
        // 创建模式：工作空间必填
        if (!workspaceValue?.trim()) {
          message.error('请选择工作空间');
          setSaving(false);
          return;
        }
        const res = await dbLoops.createLoop({
          name: basePayload.name,
          description: basePayload.description,
          workspace: workspaceValue.trim(),
          tag_ids: basePayload.tag_ids,
          icon: basePayload.icon,
          review_template_id: basePayload.review_template_id,
          limits_config: basePayload.limits_config,
          abnormal_handler_todo_id: basePayload.abnormal_handler_todo_id,
          abnormal_handler_trigger_on: basePayload.abnormal_handler_trigger_on,
        });
        message.success('环路已创建');
        onSaved(res.id);
      } else {
        // 编辑模式
        if (!loopId) return;
        await dbLoops.updateLoop(loopId, basePayload);
        message.success('已保存');
        onSaved();
      }
      onClose();
    } catch (e) {
      message.error(mode === 'create' ? '创建失败' : '保存失败，请重试');
    } finally {
      setSaving(false);
    }
  }, [form, editingTag, workspaceValue, mode, loopId, message, onSaved, onClose]);

  return (
    <>
      {/* 主表单 Drawer */}
      <Drawer
        title={mode === 'create' ? '新建环路' : '编辑 loop'}
        open={open}
        onClose={onClose}
        width={600}
        destroyOnHidden
        footer={
          <div style={{ display: 'flex', justifyContent: 'flex-end', gap: 8 }}>
            <Button onClick={onClose}>取消</Button>
            <Button type="primary" loading={saving} onClick={handleSave}>
              {mode === 'create' ? '创建' : '保存'}
            </Button>
          </div>
        }
      >
        <Form form={form} layout="vertical">
          {/* 名称：必填 */}
          <Form.Item label="名称" name="name" rules={[{ required: true, message: '名称必填' }]}>
            <Input maxLength={100} />
          </Form.Item>
          <Form.Item label="描述" name="description">
            <Input.TextArea rows={2} maxLength={500} />
          </Form.Item>
          {/* 工作空间：创建模式必填，编辑模式可选 */}
          <Form.Item
            label={<>工作空间 {mode === 'create' && <span style={{ color: '#ff4d4f' }}>*</span>}</>
          }
            tooltip="此 loop 所属的工作空间"
            rules={mode === 'create' ? [{ required: true, message: '请选择工作空间' }] : []}
          >
            <WorkspaceSelect
              value={workspaceValue}
              onChange={(v) => {
                setWorkspaceValue(v);
                form.setFieldsValue({ workspace: v });
              }}
              required={mode === 'create'}
            />
          </Form.Item>
          {tags.length > 0 && (
            <Form.Item label="标签">
              <TagCheckCardGroup
                tags={tags}
                value={editingTag}
                onChange={(val) => setEditingTag(val as number | null)}
              />
            </Form.Item>
          )}
          <Form.Item label="图标" name="icon" tooltip="预留字段, 当前仅展示">
            <Input placeholder="loop" maxLength={50} />
          </Form.Item>
          {/* 评审模板 */}
          <Form.Item
            label="评审模板"
            name="review_template_id"
            tooltip="选择用于自动评审的模板（来自设置 → 评审模板管理）。不选则使用默认模板。"
            extra={
              <Button
                type="link"
                size="small"
                icon={<PlusOutlined />}
                style={{ padding: 0, marginTop: 4 }}
                onClick={() => setCreatingTemplate(true)}
              >
                新建模板
              </Button>
            }
          >
            <Select
              allowClear
              placeholder="使用默认评审模板"
              showSearch
              optionFilterProp="label"
              options={reviewTemplateOptions.map(t => ({ value: t.id, label: t.name }))}
            />
          </Form.Item>
          {/* 全局限制 */}
          <div style={{ fontWeight: 600, fontSize: 14, marginTop: 16, marginBottom: 12, color: 'var(--color-text-secondary, #64748b)' }}>
            全局限制
          </div>
          <div style={{ background: 'var(--color-bg-elevated, #f8fafc)', padding: 12, borderRadius: 8 }}>
            <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 12 }}>
              <Form.Item label="最大执行步数" name={['max_step_executions']} tooltip="超出后自动终止 Loop（留空=不限制）">
                <InputNumber min={1} max={9999} placeholder="不限" style={{ width: '100%' }} />
              </Form.Item>
              <Form.Item label="最大 Token 数" name={['max_total_tokens']} tooltip="超出后自动终止（留空=不限制）">
                <InputNumber min={1} max={9999999999} placeholder="不限" style={{ width: '100%' }} step={1000000} />
              </Form.Item>
            </div>
          </div>
          {/* 异常处理 Todo */}
          <div style={{ fontWeight: 600, fontSize: 14, marginTop: 16, marginBottom: 8, color: 'var(--color-text-secondary, #64748b)' }}>
            异常处理
          </div>
          <div style={{ background: 'var(--color-bg-elevated, #f8fafc)', padding: 12, borderRadius: 8 }}>
            <Form.Item
              label="异常处理 Todo"
              name="abnormal_handler_todo_id"
              tooltip="当 Loop 以异常状态结束时，自动执行此 Todo 作为清理/补救措施"
            >
              <Select
                allowClear
                placeholder="不设置异常处理"
                showSearch
                optionFilterProp="label"
                options={abnormalHandlerTodoOptions.map(t => ({ value: t.id, label: t.title }))}
                style={{ width: '100%' }}
              />
            </Form.Item>
            <Form.Item
              label="触发条件"
              name="abnormal_handler_trigger_on"
              tooltip="哪些异常状态时触发异常处理 Todo"
              initialValue={['capped_step', 'capped_token', 'failed']}
              style={{ marginBottom: 0 }}
            >
              <Checkbox.Group
                options={[
                  { label: '超步数', value: 'capped_step' },
                  { label: '超 Token', value: 'capped_token' },
                  { label: '执行失败', value: 'failed' },
                ]}
              />
            </Form.Item>
          </div>
        </Form>
      </Drawer>

      {/* inline 新建评审模板的 Modal */}
      <Modal
        title="新建评审模板"
        open={creatingTemplate}
        onCancel={() => {
          newTemplateForm.resetFields();
          setCreatingTemplate(false);
        }}
        onOk={handleCreateTemplate}
        confirmLoading={creatingTemplateSaving}
        destroyOnHidden
      >
        <Form form={newTemplateForm} layout="vertical" preserve={false}>
          <Form.Item label="名称" name="name" rules={[{ required: true, message: '请输入名称' }]}>
            <Input placeholder="如：代码评审 / 文档评审" maxLength={128} />
          </Form.Item>
          <Form.Item label="描述" name="description">
            <Input placeholder="（可选）简短说明这个模板的用途" maxLength={512} />
          </Form.Item>
          <Form.Item
            label="Prompt 模板"
            name="prompt"
            rules={[{ required: true, message: '请输入 prompt 模板' }]}
            tooltip="支持占位符 {original_prompt} {original_output} {acceptance_criteria} {max_output_chars}"
          >
            <Input.TextArea rows={8} placeholder="你是一个评审师…" />
          </Form.Item>
        </Form>
      </Modal>
    </>
  );
}
