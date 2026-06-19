// Loop 钩子面板。
//
// 钩子位置 (hook_position)：
// - pre_loop / post_loop: 整个 loop 前后触发
// - pre_stage / post_stage: 某个 stage 前后触发 (必须指定 source_stage_id)
//
// 钩子本质上指向一个 todo (target_todo_id) 作为执行节点。
// 这里的目标 todo 不强制 step, 因为 hook 是一次性的副作用, 不属于"循环复用"。
// 但 v3 kind 列引入后, 约定: hook 引用 step 更符合"循环复用"语义, 引导用户。

import { useState, useCallback, useEffect } from 'react';
import {
  App as AntApp,
  Button,
  Empty,
  Modal,
  Form,
  Select,
  Switch,
  Space,
  Tag,
  Popconfirm,
  List,
  InputNumber,
} from 'antd';
import { PlusOutlined, DeleteOutlined, EditOutlined, LinkOutlined } from '@ant-design/icons';
import * as dbLoops from '@/utils/database/loops';
import * as dbExperts from '@/utils/database/steps';
import type {
  LoopHookDto,
  LoopDetail,
  CreateHookRequest,
  UpdateHookRequest,
} from '@/types/loop';
import type { Todo } from '@/types';

interface Props {
  loopId: number;
  hooks: LoopHookDto[];
  stages: Record<string, any>[];
  todoMap: LoopDetail['todo_map'];
  onChanged: () => void;
}

interface HookForm {
  hook_position: string;
  source_stage_id: number | null;
  target_todo_id: number;
  skip_if_missing: boolean;
  enabled: boolean;
  min_rating: number | null;
  unrated_policy: string;
}

const POSITION_LABEL: Record<string, string> = {
  pre_loop: 'loop 前',
  post_loop: 'loop 后',
  pre_stage: '阶段前',
  post_stage: '阶段后',
};

export function LoopHooksPanel({ loopId, hooks, stages, todoMap, onChanged }: Props) {
  const { message } = AntApp.useApp();
  const [editing, setEditing] = useState<{ hook: LoopHookDto } | null>(null);
  const [creating, setCreating] = useState(false);
  const [form] = Form.useForm<HookForm>();
  const [steps, setExperts] = useState<Todo[]>([]);
  const [stepsLoading, setExpertsLoading] = useState(false);

  // 加载专家候选
  useEffect(() => {
    setExpertsLoading(true);
    dbExperts.listStepCandidates()
      .then(setExperts)
      .catch(() => setExperts([]))
      .finally(() => setExpertsLoading(false));
  }, []);

  const handleOpenCreate = useCallback(() => {
    form.resetFields();
    form.setFieldsValue({
      hook_position: 'pre_loop',
      source_stage_id: null,
      target_todo_id: undefined,
      skip_if_missing: false,
      enabled: true,
      min_rating: null,
      unrated_policy: 'skip',
    });
    setCreating(true);
    setEditing(null);
  }, [form]);

  const handleOpenEdit = useCallback((hook: LoopHookDto) => {
    form.setFieldsValue({
      hook_position: hook.hook_position,
      source_stage_id: hook.source_stage_id,
      target_todo_id: hook.target_todo_id,
      skip_if_missing: hook.skip_if_missing,
      enabled: hook.enabled,
      min_rating: hook.min_rating,
      unrated_policy: hook.unrated_policy,
    });
    setEditing({ hook });
    setCreating(false);
  }, [form]);

  const handleClose = useCallback(() => {
    setCreating(false);
    setEditing(null);
  }, []);

  const handleSubmit = useCallback(async () => {
    const values = await form.validateFields();
    // 后端要求: pre_stage / post_stage 必须指定 source_stage_id
    if ((values.hook_position === 'pre_stage' || values.hook_position === 'post_stage')
        && values.source_stage_id === null) {
      message.error('pre_stage / post_stage 必须指定源阶段');
      return;
    }
    try {
      if (editing) {
        await dbLoops.updateHook(loopId, editing.hook.id, {
          hook_position: values.hook_position,
          source_stage_id: values.source_stage_id,
          target_todo_id: values.target_todo_id,
          skip_if_missing: values.skip_if_missing,
          enabled: values.enabled,
          min_rating: values.min_rating,
          unrated_policy: values.unrated_policy,
        } as UpdateHookRequest);
        message.success('已更新');
      } else {
        await dbLoops.createHook(loopId, {
          hook_position: values.hook_position,
          source_stage_id: values.source_stage_id,
          target_todo_id: values.target_todo_id,
          skip_if_missing: values.skip_if_missing,
          enabled: values.enabled,
          min_rating: values.min_rating,
          unrated_policy: values.unrated_policy,
        } as CreateHookRequest);
        message.success('已添加');
      }
      handleClose();
      onChanged();
    } catch {
      // ignore
    }
  }, [form, editing, loopId, message, handleClose, onChanged]);

  const handleDelete = useCallback(async (id: number) => {
    try {
      await dbLoops.deleteHook(loopId, id);
      message.success('已删除');
      onChanged();
    } catch {
      // ignore
    }
  }, [loopId, message, onChanged]);

  return (
    <div className="loop-hooks-panel">
      <div style={{ marginBottom: 12, display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
        <span style={{ color: 'var(--color-text-secondary, #475569)' }}>钩子在 loop / 阶段的前后触发, 用于插入副作用</span>
        <Button type="primary" size="small" icon={<PlusOutlined />} onClick={handleOpenCreate}>
          新增钩子
        </Button>
      </div>

      {hooks.length === 0 ? (
        <Empty description="暂无钩子" />
      ) : (
        <List
          dataSource={hooks}
          renderItem={(h) => {
            const sourceStage = h.source_stage_id
              ? stages.find(s => s.id === h.source_stage_id)
              : null;
            const target = todoMap[h.target_todo_id];
            return (
              <List.Item
                actions={[
                  <Button key="edit" size="small" type="text" icon={<EditOutlined />} onClick={() => handleOpenEdit(h)} />,
                  <Popconfirm key="del" title="删除钩子?" onConfirm={() => handleDelete(h.id)}>
                    <Button size="small" type="text" danger icon={<DeleteOutlined />} />
                  </Popconfirm>,
                ]}
              >
                <List.Item.Meta
                  avatar={<LinkOutlined style={{ color: 'var(--color-info, #3b82f6)' }} />}
                  title={
                    <Space wrap>
                      <Tag color="blue">{POSITION_LABEL[h.hook_position] ?? h.hook_position}</Tag>
                      {sourceStage && <Tag>源: {sourceStage.name}</Tag>}
                      {target ? (
                        <Tag color="purple">目标: #{target.id} {target.title}</Tag>
                      ) : (
                        <Tag color="red">目标 todo #{h.target_todo_id} 不存在</Tag>
                      )}
                      {!h.enabled && <Tag>已禁用</Tag>}
                    </Space>
                  }
                  description={
                    <Space size="small">
                      {h.skip_if_missing && <Tag>目标缺失时跳过</Tag>}
                      {h.min_rating !== null && <Tag color="blue">评分 ≥ {h.min_rating}</Tag>}
                    </Space>
                  }
                />
              </List.Item>
            );
          }}
        />
      )}

      <Modal
        title={editing ? '编辑钩子' : '新增钩子'}
        open={creating || editing !== null}
        onCancel={handleClose}
        onOk={handleSubmit}
        okText="保存"
        cancelText="取消"
        width={560}
        destroyOnClose
      >
        <Form form={form} layout="vertical">
          <Form.Item label="位置" name="hook_position" rules={[{ required: true }]}>
            <Select
              options={[
                { value: 'pre_loop', label: 'pre_loop (整个 loop 前)' },
                { value: 'post_loop', label: 'post_loop (整个 loop 后)' },
                { value: 'pre_stage', label: 'pre_stage (某阶段前)' },
                { value: 'post_stage', label: 'post_stage (某阶段后)' },
              ]}
            />
          </Form.Item>
          <Form.Item
            label="源阶段"
            name="source_stage_id"
            tooltip="pre_stage / post_stage 必须指定, pre_loop / post_loop 留空"
          >
            <Select
              allowClear
              placeholder="选择源阶段"
              options={stages.map(s => ({ value: s.id, label: `${s.order_index + 1}. ${s.name}` }))}
            />
          </Form.Item>
          <Form.Item label="目标 todo" name="target_todo_id" rules={[{ required: true, message: '请选择目标' }]}>
            <Select
              placeholder="选择 step todo"
              loading={stepsLoading}
              showSearch
              optionFilterProp="label"
              options={steps.map(t => ({
                value: t.id,
                label: `#${t.id} ${t.title}`,
              }))}
            />
          </Form.Item>
          <Space size="middle">
            <Form.Item label="评分阈值" name="min_rating">
              <InputNumber min={1} max={5} style={{ width: 100 }} />
            </Form.Item>
            <Form.Item label="未评分策略" name="unrated_policy" style={{ minWidth: 120 }}>
              <Select
                options={[
                  { value: 'skip', label: '跳过' },
                  { value: 'continue', label: '继续' },
                ]}
              />
            </Form.Item>
          </Space>
          <Space size="large">
            <Form.Item label="目标缺失时跳过" name="skip_if_missing" valuePropName="checked">
              <Switch />
            </Form.Item>
            <Form.Item label="启用" name="enabled" valuePropName="checked">
              <Switch />
            </Form.Item>
          </Space>
        </Form>
      </Modal>
    </div>
  );
}
