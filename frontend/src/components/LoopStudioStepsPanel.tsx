// Loop Studio 执行环节面板：DAG 流程图布局，支持控制流配置。
//
// 使用 dagre 自动布局，SVG 绘制有向边，支持条件分支可视化。
//
// 「关联环节」下拉按 loop 所属工作空间过滤候选 todo：
// - 有 workspace 时调 db.getAllTodos(workspace_id)，只显示同工作空间下的事项
// - 无 workspace 时不过滤（保留旧行为，不丢已有选项）
// - 编辑环节时，已关联的 todo 若不在过滤结果里，额外补回，
//   避免「选了别的工作空间的 todo 就再看不到」的边界陷阱

import { useState, useCallback } from 'react';
import {
  App as AntApp, Button, Modal, Form, Input, InputNumber, Select, Switch, Popconfirm, Empty, Space,
} from 'antd';
import { DeleteOutlined, CloseOutlined } from '@ant-design/icons';
import * as dbLoops from '@/utils/database/loops';
import * as db from '@/utils/database';
import {
  getWorkspaceDisplayName,
  useProjectDirectories,
} from '@/utils/workspaceDisplay';
import type { LoopStepDto, CreateLoopStepRequest } from '@/types/loop';
import type { Todo } from '@/types';
import { LoopFlowGraph } from '@/components/loop-flow/LoopFlowGraph';

interface StepsPanelProps {
  loopId: number;
  steps: LoopStepDto[];
  onChanged: () => void;
  /** Loop 的最大执行步数限制，来自 loop.limits_config.max_step_executions */
  maxStepExecutions?: number | null;
  /** Loop 的最大 Token 数限制，来自 loop.limits_config.max_total_tokens */
  maxTotalTokens?: number | null;
  /** Loop 所属工作空间 ID（project_directories.id，唯一键）。
   *  用于过滤「关联环节」候选 todo，并把 id 转成 name 展示给用户。 */
  workspaceId?: number | null;
}

export function LoopStepsPanel({ loopId, steps, onChanged, maxStepExecutions, maxTotalTokens, workspaceId }: StepsPanelProps) {
  const { message } = AntApp.useApp();
  // 工作空间目录（低基数集合，一次性拉取，避免每次打开 modal 都重复请求）
  const { dirs: projectDirs } = useProjectDirectories();

  // Modal 状态
  const [modalOpen, setModalOpen] = useState(false);
  const [editingStep, setEditingStep] = useState<LoopStepDto | null>(null);
  const [saving, setSaving] = useState(false);
  const [candidates, setCandidates] = useState<Todo[]>([]);
  const [form] = Form.useForm<CreateLoopStepRequest & { todo_title: string }>();

  // 打开新增 Modal
  const handleOpenAdd = useCallback(async () => {
    setEditingStep(null);
    form.resetFields();
    await loadCandidatesForCurrentLoop(null);
    setModalOpen(true);
  }, [form, workspaceId]);

  // 点击流程图节点打开编辑
  const handleSelectStep = useCallback(async (step: LoopStepDto) => {
    setEditingStep(step);
    await loadCandidatesForCurrentLoop(step);
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
      review_type: step.review_type ?? 'ai',
    });
    setModalOpen(true);
  }, [form, workspaceId]);

  /**
   * 按 loop 所属工作空间加载候选 todo 列表。
   *
   * 设计取舍：
   * - workspaceId 非空 → 调 getAllTodos(workspaceId) 只拿同空间下的事项，
   *   避免误把别的工作空间的事项串到当前 loop 的环节里。
   * - workspaceId 为空 → 回退到不过滤（getAllTodos()），保持旧行为，不丢选项。
   * - 编辑环节时，已关联的 todo 若不在过滤结果里，额外补回，保证用户至少能看到
   *   当前环节指向的 todo 仍然可选（避免「换工作空间 → 旧 todo 从下拉消失 → 选不回去」
   *   的死循环）。
   *
   * 失败一律静默回退为空数组，不阻塞打开 modal；空态由 Select 的 notFoundContent 兜底。
   */
  const loadCandidatesForCurrentLoop = useCallback(async (stepForEdit: LoopStepDto | null) => {
    try {
      // workspaceId 必须有值才能查 todos（v1 纯 workspace-scoped）
      if (workspaceId == null) {
        setCandidates([]);
        return;
      }
      const list = await db.getAllTodos(workspaceId);
      // 编辑模式下若已绑定的 todo 不在过滤结果里，附加一次单条查询补回选项，
      // 确保用户依然能在下拉里看到/选中当前关联的 todo。
      // 不附加 workspaceId 过滤，因为这条记录可能原本属于其他工作空间。
      if (stepForEdit && !list.some(t => t.id === stepForEdit.todo_id)) {
        try {
          const current = await db.getTodo(workspaceId, stepForEdit.todo_id);
          if (current) setCandidates([...list, current]);
          else setCandidates(list);
        } catch {
          setCandidates(list);
        }
      } else {
        setCandidates(list);
      }
    } catch {
      // 拉取失败兜底为空数组；用户看到「暂无待关联的事项」即可，不阻塞流程
      setCandidates([]);
    }
  }, [workspaceId]);

  // 保存
  const handleSave = useCallback(async () => {
    const values = await form.validateFields();
    // 评分不通过时跳转到自身（重试），需要至少有一个兜底限制
    if (
      editingStep &&
      values.on_rating_fail === 'goto' &&
      values.fail_goto_step_id === editingStep.id &&
      !maxStepExecutions && !maxTotalTokens
    ) {
      message.error('跳转到自身需要设置「最大执行步数」或「最大 Token 数」兜底，请在 Loop 基础信息中配置');
      return;
    }
    setSaving(true);
    try {
      if (editingStep) {
        await dbLoops.updateLoopStep(workspaceId ?? 0, loopId, editingStep.id, {
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
          review_type: values.review_type ?? 'ai',
        });
        message.success('环节已更新');
      } else {
        await dbLoops.createLoopStep(workspaceId ?? 0, loopId, {
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
          review_type: values.review_type ?? 'ai',
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
  }, [form, loopId, editingStep, message, onChanged, maxStepExecutions, maxTotalTokens]);

  // 删除环节
  const handleDelete = useCallback(async (stepId: number) => {
    try {
      await dbLoops.deleteLoopStep(workspaceId ?? 0, loopId, stepId);
      message.success('环节已删除');
      onChanged();
    } catch {
      // 后端错误已有 message
    }
  }, [loopId, message, onChanged]);

  // 选择 step 后自动填充 name
  const handleTodoChange = useCallback((stepId: number) => {
    const selected = candidates.find(c => c.id === stepId);
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
            <Button type="text" size="small" icon={<CloseOutlined />}
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
              <Button type="text" size="small" icon={<DeleteOutlined />} style={{ fontSize: 12 }}>
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
              placeholder={workspaceId != null ? `仅显示「${getWorkspaceDisplayName(projectDirs, workspaceId)}」工作空间下的事项` : '选择已有的环节'}
              optionFilterProp="label"
              onChange={handleTodoChange}
              options={candidates.map(c => ({
                label: `#${c.id} ${c.title}`,
                value: c.id,
              }))}
              notFoundContent={
                <Empty description={workspaceId != null ? `「${getWorkspaceDisplayName(projectDirs, workspaceId)}」下暂无待关联的事项` : '暂无待关联的事项'} image={Empty.PRESENTED_IMAGE_SIMPLE} />
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
          <Form.Item
            label="评审类型"
            name="review_type"
            initialValue="ai"
            tooltip="AI 自动评审：执行完后 AI 自动打分；人工审批：执行完后暂停等待人工打分"
          >
            <Select>
              <Select.Option value="ai">🤖 AI 自动评审</Select.Option>
              <Select.Option value="human">👤 人工审批</Select.Option>
            </Select>
          </Form.Item>
          <Form.Item label="评分阈值" name="min_rating" tooltip="启用后根据评审类型进行评分，低于此值视为不通过（0-100，留空=不启用）">
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
                  placeholder="选择目标环节（选择自身=重试）"
                  options={steps
                    .map(s => ({ label: s.id === editingStep?.id ? `${s.name} (#${s.id}) ⬅ 重试` : `${s.name} (#${s.id})`, value: s.id }))}
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
