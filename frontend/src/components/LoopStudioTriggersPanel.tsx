// Loop 触发条件面板 (重做: 内联 toggle 列表)。
//
// 对齐参考设计: 全 8 种触发类型全部展示, 每行带 toggle 开关,
// 不再需要先点「新增触发器」选类型。打开 toggle 时弹配置 modal (cron 表达式 / webhook id 等)。
// 已存在的触发器可点击行名进入编辑/删除。
//
// 触发器类型: manual | cron | webhook | feishu_message | feishu_command
//            | todo_completed | todo_state_changed

import { useState, useCallback, useMemo } from 'react';
import {
  App as AntApp,
  Button,
  Modal,
  Form,
  Input,
  Select,
  Switch,
  Tooltip,
} from 'antd';
import {
  ApiOutlined,
  ClockCircleOutlined,
  PlayCircleOutlined,
  MessageOutlined,
  ThunderboltOutlined,
  CheckCircleOutlined,
  DeleteOutlined,
  EditOutlined,
} from '@ant-design/icons';
import * as dbLoops from '@/utils/database/loops';
import type { LoopTriggerDto, CreateTriggerRequest, UpdateTriggerRequest } from '@/types/loop';
import { formatRelativeTime } from '@/utils/datetime';

interface Props {
  loopId: number;
  triggers: LoopTriggerDto[];
  onChanged: () => void;
}

// 触发器类型 → 元数据, 集中在一处; export 让其他组件 (e.g. LoopStudio) 复用
export const TRIGGER_META: Record<string, { icon: React.ReactNode; label: string; desc: string; configHint: string }> = {
  manual: {
    icon: <PlayCircleOutlined />,
    label: '手动触发',
    desc: '通过「触发」按钮主动启动',
    configHint: '无需配置',
  },
  cron: {
    icon: <ClockCircleOutlined />,
    label: '定时调度',
    desc: '按 cron 表达式周期性触发',
    configHint: '{"expr":"0 9 * * *","tz":"Asia/Shanghai"}',
  },
  webhook: {
    icon: <ApiOutlined />,
    label: 'Webhook',
    desc: '外部 HTTP 回调触发',
    configHint: '{"webhook_id":"any-string-you-like","method":"POST"}',
  },
  feishu_message: {
    icon: <MessageOutlined />,
    label: '飞书消息',
    desc: '收到飞书消息时触发',
    configHint: '{"match":"关键字"}',
  },
  feishu_command: {
    icon: <ThunderboltOutlined />,
    label: '飞书指令',
    desc: '飞书内指定指令触发',
    configHint: '{"command":"/review"}',
  },
  todo_completed: {
    icon: <CheckCircleOutlined />,
    label: 'Todo 完成',
    desc: '某个 todo 完成时触发',
    configHint: '{"todo_id":1}',
  },
  todo_state_changed: {
    icon: <CheckCircleOutlined />,
    label: 'Todo 状态变更',
    desc: '任意 todo 状态变化时触发',
    configHint: '{"from":"pending","to":"completed"}',
  },
};

// 内联配置 modal 的表单 schema (不同 trigger_type 公用)
interface TriggerForm {
  config: string;
  priority: number;
}

export function LoopTriggersPanel({ loopId, triggers, onChanged }: Props) {
  const { message } = AntApp.useApp();
  // 当前正在配置的 trigger_type (open=true 时): null=关闭
  const [configuring, setConfiguring] = useState<{ type: string; existing: LoopTriggerDto | null } | null>(null);
  const [saving, setSaving] = useState(false);
  const [form] = Form.useForm<TriggerForm>();

  // 按 type 分组索引已存在的 trigger (1 个 type 最多 1 个 trigger, 简化模型)
  const byType = useMemo(() => {
    const m = new Map<string, LoopTriggerDto>();
    for (const t of triggers) m.set(t.trigger_type, t);
    return m;
  }, [triggers]);

  // 切换 enabled 状态 (用户点 toggle 的非「新增」路径)
  const handleToggleEnabled = useCallback(async (t: LoopTriggerDto, enabled: boolean) => {
    try {
      await dbLoops.updateTrigger(loopId, t.id, {
        trigger_type: t.trigger_type,
        config: t.config,
        enabled,
        priority: t.priority,
      } as UpdateTriggerRequest);
      onChanged();
    } catch {
      // ignore: 拦截器已弹错
    }
  }, [loopId, onChanged]);

  // 关闭配置 modal
  const closeConfig = useCallback(() => {
    setConfiguring(null);
    form.resetFields();
  }, [form]);

  // 打开新增配置 modal (toggle 从 off 变 on, 没有已存在记录)
  const openCreateConfig = useCallback((type: string) => {
    const meta = TRIGGER_META[type];
    if (!meta) return;
    form.setFieldsValue({ config: meta.configHint, priority: 0 });
    setConfiguring({ type, existing: null });
  }, [form]);

  // 打开编辑配置 modal (点击已存在 trigger 的行名)
  const openEditConfig = useCallback((t: LoopTriggerDto) => {
    form.setFieldsValue({ config: t.config, priority: t.priority });
    setConfiguring({ type: t.trigger_type, existing: t });
  }, [form]);

  // 提交配置 (新建 / 更新二选一)
  const handleSaveConfig = useCallback(async () => {
    if (!configuring) return;
    const values = await form.validateFields();
    setSaving(true);
    try {
      if (configuring.existing) {
        await dbLoops.updateTrigger(loopId, configuring.existing.id, {
          trigger_type: configuring.type,
          config: values.config || '{}',
          enabled: configuring.existing.enabled,
          priority: values.priority || 0,
        } as UpdateTriggerRequest);
        message.success('已更新');
      } else {
        await dbLoops.createTrigger(loopId, {
          trigger_type: configuring.type,
          config: values.config || '{}',
          enabled: true,
          priority: values.priority || 0,
        } as CreateTriggerRequest);
        message.success(`已添加「${TRIGGER_META[configuring.type]?.label ?? configuring.type}」触发器`);
      }
      closeConfig();
      onChanged();
    } catch {
      // ignore
    } finally {
      setSaving(false);
    }
  }, [configuring, form, loopId, message, closeConfig, onChanged]);

  // 删除已有 trigger
  const handleDelete = useCallback(async (id: number) => {
    try {
      await dbLoops.deleteTrigger(loopId, id);
      message.success('已删除');
      onChanged();
      // 如果当前正在编辑的就是被删的, 关闭 modal
      if (configuring?.existing?.id === id) closeConfig();
    } catch {
      // ignore
    }
  }, [loopId, message, onChanged, configuring, closeConfig]);

  // 渲染一行 trigger: 图标 + label + desc + (已启用 toggle / 新增 toggle / 编辑按钮)
  const renderRow = (type: string) => {
    const meta = TRIGGER_META[type];
    if (!meta) return null;
    const existing = byType.get(type);
    const isOn = !!existing;
    const isEnabled = existing?.enabled ?? false;

    return (
      <div
        key={type}
        style={{
          display: 'flex', alignItems: 'center', gap: 12,
          padding: '10px 12px',
          borderRadius: 8,
          background: isOn
            ? 'var(--color-primary-bg, #f0f9ff)'
            : 'var(--color-bg-elevated, #ffffff)',
          border: `1px solid ${isOn
            ? 'color-mix(in srgb, var(--color-primary, #0891b2) 50%, transparent)'
            : 'var(--color-border, #e2e8f0)'}`,
          marginBottom: 8,
          transition: 'border-color 200ms, background 200ms',
        }}
      >
        <span style={{
          display: 'inline-flex', alignItems: 'center', justifyContent: 'center',
          width: 28, height: 28, borderRadius: 6,
          background: isOn ? 'var(--color-primary, #0891b2)' : 'var(--color-bg-hover, #f1f5f9)',
          color: isOn ? '#fff' : 'var(--color-text-secondary, #475569)',
          fontSize: 14,
        }}>{meta.icon}</span>
        <div style={{ flex: 1, minWidth: 0 }}>
          <div
            style={{ fontSize: 13, fontWeight: 500, cursor: existing ? 'pointer' : 'default', color: 'var(--color-text, #0f172a)' }}
            onClick={() => existing && openEditConfig(existing)}
          >
            {meta.label}
            {existing && (
              <EditOutlined style={{ marginLeft: 6, fontSize: 11, color: 'var(--color-text-tertiary, #94a3b8)' }} />
            )}
          </div>
          <div style={{ fontSize: 11, color: 'var(--color-text-tertiary, #94a3b8)', marginTop: 2 }}>
            {meta.desc}
            {existing && existing.created_at && (
              <span style={{ marginLeft: 8 }}>· 创建于 {formatRelativeTime(existing.created_at)}</span>
            )}
          </div>
        </div>
        {isOn ? (
          <Tooltip title={isEnabled ? '点击禁用' : '点击启用'}>
            <Switch size="small" checked={isEnabled} onChange={(v) => handleToggleEnabled(existing!, v)} />
          </Tooltip>
        ) : (
          <Button size="small" type="primary" onClick={() => openCreateConfig(type)}>
            启用
          </Button>
        )}
      </div>
    );
  };

  // 7 种 trigger type 全部展示 (manual / cron / webhook / feishu_message / feishu_command / todo_completed / todo_state_changed)
  const allTypes = Object.keys(TRIGGER_META);

  return (
    <div className="loop-triggers-panel">
      <div style={{ marginBottom: 12, fontSize: 12, color: 'var(--color-text-secondary, #475569)' }}>
        触发条件决定 loop 在何时被启动 · 当前已启用 {triggers.filter(t => t.enabled).length} / 共 {allTypes.length} 条
      </div>

      {/* 不再单独显示 Empty 状态, 下方 8 行就是「可启用项」, 已足够表达空态语义 */}
      {allTypes.map(renderRow)}

      {/* 配置 modal (新增 / 编辑通用) */}
      <Modal
        title={configuring?.existing ? `编辑「${TRIGGER_META[configuring.type]?.label ?? configuring.type}」` : `启用「${configuring ? (TRIGGER_META[configuring.type]?.label ?? '') : ''}」`}
        open={configuring !== null}
        onCancel={closeConfig}
        onOk={handleSaveConfig}
        okText={configuring?.existing ? '保存' : '启用'}
        cancelText="取消"
        confirmLoading={saving}
        destroyOnClose
        footer={configuring?.existing ? [
          <Button key="del" danger icon={<DeleteOutlined />} onClick={() => handleDelete(configuring.existing!.id)}>
            删除
          </Button>,
          <Button key="cancel" onClick={closeConfig}>取消</Button>,
          <Button key="ok" type="primary" loading={saving} onClick={handleSaveConfig}>保存</Button>,
        ] : undefined}
      >
        {configuring && (
          <Form form={form} layout="vertical">
            <Form.Item
              label="配置 (JSON)"
              name="config"
              tooltip={TRIGGER_META[configuring.type]?.configHint}
            >
              <Input.TextArea
                rows={5}
                placeholder={TRIGGER_META[configuring.type]?.configHint}
                style={{ fontFamily: 'monospace', fontSize: 12 }}
              />
            </Form.Item>
            <Form.Item label="优先级" name="priority" tooltip="数值越大越优先派发 (多个触发器并存时)">
              <Select
                options={[
                  { value: 0, label: '0 (默认)' },
                  { value: 5, label: '5' },
                  { value: 10, label: '10 (高)' },
                ]}
              />
            </Form.Item>
          </Form>
        )}
      </Modal>
    </div>
  );
}