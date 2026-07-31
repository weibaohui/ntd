// 新建任务 Modal。
// 形态参考 LoopFormModal / SmartCreateModal：
//   Modal + Form + TextArea(需求) + Select(工艺环路) + 提交按钮。
// API：bundledApi.createTask(requirement, loopId, wsId)
// 提交成功后调 onCreated，宿主关闭 Modal 并刷新列表。

import { useEffect, useState } from 'react';
import { Modal, Input, Select, Form, message } from 'antd';
import { ThunderboltOutlined } from '@ant-design/icons';
import bundledApi from '@/api/bundled';
import type { LoopLite } from '@/components/tasks/constants';
// loopOptionLabel：049 统一的选项文案拼装入口，弹窗不内联模板字符串，
// 防止多处拼装口径漂移（回退规则只维护一份）。
import { loopOptionLabel } from '@/components/tasks/constants';

interface CreateTaskModalProps {
  open: boolean;
  workspaceId: number;
  loops: LoopLite[];
  onCreated: () => void;
  onCancel: () => void;
}

/**
 * 新建任务 Modal。
 *
 * 处理流程：
 *   1. 用户填需求 + 选工艺环路（必填）。
 *   2. 点「开始执行」→ createTask API。
 *   3. 成功 → message.success + onCreated（宿主关 Modal + 刷新列表）。
 *   4. 失败 → message.error，Modal 不关，让用户重试。
 *
 * Form.Item name 必须与表单字段一致：
 *   requirement: TextArea
 *   loopId: Select
 *
 * Form 用 antd 受控模式，createModalOpen 关闭时手动 resetFields。
 */
export function CreateTaskModal({
  open,
  workspaceId,
  loops,
  onCreated,
  onCancel,
}: CreateTaskModalProps) {
  // antd Form 实例：用 Form.useForm 获取，便于关闭时重置。
  const [form] = Form.useForm<{ requirement: string; loopId: number }>();
  // 提交 loading 态：防止重复点击。
  const [submitting, setSubmitting] = useState(false);

  // open 变为 false 时重置表单字段，避免下次打开残留上次输入。
  // useEffect 依赖 open，只在 open 变化时触发。
  useEffect(() => {
    if (!open) {
      form.resetFields();
    }
  }, [open, form]);

  // 提交处理：
  //   1. form.validateFields 触发 antd 表单校验（required）。
  //   2. 校验通过 → createTask API。
  //   3. 成功 → message.success + onCreated。
  //   4. 失败 → message.error，不关 Modal。
  const handleSubmit = async () => {
    try {
      const values = await form.validateFields();
      setSubmitting(true);
      const result = await bundledApi.createTask(
        values.requirement,
        values.loopId,
        workspaceId,
      );
      message.success(`任务已创建，执行 #${result.execution_id}`);
      onCreated();
    } catch (err) {
      // validateFields 失败时不报错；只有 API 失败才报错。
      if (err instanceof Error) {
        message.error(`创建任务失败：${err.message}`);
      }
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <Modal
      title={
        <span>
          <ThunderboltOutlined style={{ marginRight: 8 }} />
          新建任务
        </span>
      }
      open={open}
      onCancel={onCancel}
      onOk={handleSubmit}
      confirmLoading={submitting}
      okText="开始执行"
      cancelText="取消"
      width={560}
      destroyOnClose
      data-testid="create-task-modal"
    >
      <Form
        form={form}
        layout="vertical"
        requiredMark
      >
        <Form.Item
          name="requirement"
          label="需求描述"
          rules={[
            { required: true, message: '请输入需求描述' },
            { min: 4, message: '需求描述至少 4 个字符' },
          ]}
        >
          <Input.TextArea
            placeholder="我想做什么？例如：把这段 Rust 代码改成异步实现"
            rows={4}
            maxLength={2000}
            showCount
            data-testid="create-task-requirement"
          />
        </Form.Item>

        <Form.Item
          name="loopId"
          label="工艺环路"
          rules={[{ required: true, message: '请选择工艺环路' }]}
          extra={
            loops.length === 0
              ? '当前工作空间暂无工艺环路，请先在「环路」页创建'
              : undefined
          }
        >
          <Select
            placeholder="选择一个工艺环路"
            options={loops.map((l) => ({
              // 049：label 由 loopOptionLabel 拼装，格式
              // 「#<环路ID> 环路名称（#工艺ID 工艺名称 工艺版本）」，
              // 同名环路靠 ID 区分，同 ID 环路靠工艺名+版本区分来源。
              label: loopOptionLabel(l),
              value: l.id,
            }))}
            notFoundContent="暂无可用工艺环路"
            data-testid="create-task-loop-select"
          />
        </Form.Item>
      </Form>
    </Modal>
  );
}
