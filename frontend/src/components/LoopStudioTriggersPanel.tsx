// Loop 触发条件面板。
//
// 设计目标：7 种触发类型（manual/cron/feishu_message/feishu_command/
// todo_completed/todo_state_changed/tag_added）全部以行内 toggle 展示。
// 每种类型有独立的配置弹窗，不再需要用户手写 JSON。
//
// 定时调度（cron）使用 react-js-cron + CronPresetSelect（和 todo 定时调度一致）。
// Todos / Tags / Feishu bots 等通过 Select 下拉选取已有数据。

import { useState, useCallback, useMemo, useEffect } from 'react';
import {
  App as AntApp,
  Button,
  Modal,
  Form,
  Input,
  Select,
  Switch,
  Tooltip,
  Tag as AntTag,
} from 'antd';
import {
  ClockCircleOutlined,
  PlayCircleOutlined,
  MessageOutlined,
  ThunderboltOutlined,
  CheckCircleOutlined,
  SyncOutlined,
  TagOutlined,
  DeleteOutlined,
  EditOutlined,
} from '@ant-design/icons';
import * as dbLoops from '@/utils/database/loops';
import * as db from '@/utils/database';
import type { LoopTriggerDto, CreateTriggerRequest, UpdateTriggerRequest } from '@/types/loop';
import type { Todo, Tag } from '@/types';
import { formatRelativeTime } from '@/utils/datetime';
import { CronPresetSelect } from '@/components/CronPresetSelect';
import { Cron } from 'react-js-cron';
import 'react-js-cron/dist/styles.css';
import { CRON_ZH_LOCALE, cronTo5, cronTo6 } from '@/utils/cron';

interface Props {
  loopId: number;
  triggers: LoopTriggerDto[];
  onChanged: () => void;
}

// ====== 触发器元数据 ======

export const TRIGGER_META: Record<string, {
  icon: React.ReactNode;
  label: string;
  desc: string;
  // 不需要配置（manual 等）
  noConfig?: boolean;
}> = {
  manual: {
    icon: <PlayCircleOutlined />,
    label: '手动触发',
    desc: '通过「触发」按钮主动启动',
    noConfig: true,
  },
  cron: {
    icon: <ClockCircleOutlined />,
    label: '定时调度',
    desc: '按 cron 表达式周期性触发',
  },
  feishu_message: {
    icon: <MessageOutlined />,
    label: '飞书消息',
    desc: '收到飞书消息时触发',
  },
  feishu_command: {
    icon: <ThunderboltOutlined />,
    label: '飞书指令',
    desc: '飞书内指定指令触发',
  },
  todo_completed: {
    icon: <CheckCircleOutlined />,
    label: 'Todo 完成',
    desc: '某个 todo 完成时触发',
  },
  todo_state_changed: {
    icon: <SyncOutlined />,
    label: 'Todo 状态变更',
    desc: '某个 todo 状态变化时触发',
  },
  tag_added: {
    icon: <TagOutlined />,
    label: '标签添加',
    desc: '某个标签被添加到 todo 时触发',
  },
};

// ====== 各触发类型的专用配置组件 ======

/**
 * 定时调度配置：复用 todo 定时调度的 CronPresetSelect + react-js-cron。
 * 存储格式（保持与后端一致）：{"cron":"0 0 9 * * *","timezone":"Asia/Shanghai"}
 */
function CronConfigForm({ value, onChange }: {
  value: string;
  onChange: (json: string) => void;
}) {
  // 解析当前值，提取 cron 和 timezone
  const parsed = useMemo(() => {
    try {
      const v = JSON.parse(value || '{}');
      return {
        cron: v.cron || '0 0 9 * * *',
        timezone: v.timezone || 'Asia/Shanghai',
      };
    } catch {
      return { cron: '0 0 9 * * *', timezone: 'Asia/Shanghai' };
    }
  }, [value]);

  const [cronExpr, setCronExpr] = useState(parsed.cron);
  const [timezone, setTimezone] = useState(parsed.timezone);

  // 同步回外层
  const sync = useCallback((c: string, tz: string) => {
    onChange(JSON.stringify({ cron: c, timezone: tz }));
  }, [onChange]);

  useEffect(() => { sync(cronExpr, timezone); }, [cronExpr, timezone, sync]);

  return (
    <div>
      {/* 快速预设选择 */}
      <CronPresetSelect
        value={cronExpr}
        onChange={(val) => setCronExpr(val)}
      />
      {/* react-js-cron 图形化编辑器 */}
      <div style={{ marginTop: 8, marginBottom: 12 }}>
        <Cron
          value={cronTo5(cronExpr)}
          setValue={(val: string) => setCronExpr(cronTo6(val))}
          locale={CRON_ZH_LOCALE}
          defaultPeriod="hour"
          humanizeLabels
          allowClear={false}
        />
      </div>
      {/* 时区选择 */}
      <Form.Item label="时区" tooltip="cron 表达式在该时区的本地时间执行">
        <Select
          value={timezone}
          onChange={(v) => setTimezone(v)}
          showSearch
          options={[
            { value: 'Asia/Shanghai', label: 'Asia/Shanghai (UTC+8)' },
            { value: 'Asia/Tokyo', label: 'Asia/Tokyo (UTC+9)' },
            { value: 'America/New_York', label: 'America/New_York (UTC-5/-4)' },
            { value: 'America/Los_Angeles', label: 'America/Los_Angeles (UTC-8/-7)' },
            { value: 'Europe/London', label: 'Europe/London (UTC+0/+1)' },
            { value: 'Europe/Berlin', label: 'Europe/Berlin (UTC+1/+2)' },
            { value: 'UTC', label: 'UTC' },
          ]}
        />
      </Form.Item>
    </div>
  );
}

/**
 * Todo 选择器（todo_completed / todo_state_changed 共用）。
 * 存储格式：{"todo_id": 7}
 */
function TodoSelectorForm({ value, onChange }: {
  value: string;
  onChange: (json: string) => void;
}) {
  const [todos, setTodos] = useState<Todo[]>([]);
  const [loading, setLoading] = useState(false);

  const parsed = useMemo(() => {
    try {
      const v = JSON.parse(value || '{}');
      return { todo_id: v.todo_id ?? null };
    } catch {
      return { todo_id: null };
    }
  }, [value]);

  const [selectedId, setSelectedId] = useState<number | null>(parsed.todo_id);

  // 加载所有 todo 列表（items + steps）
  useEffect(() => {
    setLoading(true);
    db.getAllTodos()
      .then((list) => setTodos(list))
      .catch(() => { /* 静默 */ })
      .finally(() => setLoading(false));
  }, []);

  const handleChange = useCallback((id: number | null) => {
    setSelectedId(id);
    if (id) {
      onChange(JSON.stringify({ todo_id: id }));
    } else {
      onChange('{}');
    }
  }, [onChange]);

  return (
    <Form.Item label="选择 Todo" tooltip="该 Todo 的状态变化将触发 Loop">
      <Select
        value={selectedId}
        onChange={handleChange}
        loading={loading}
        allowClear
        placeholder="选择一个 todo"
        showSearch
        filterOption={(input, option) =>
          (option?.label ?? '').toLowerCase().includes(input.toLowerCase())
        }
        options={todos.map((t) => ({
          value: t.id,
          label: t.title,
        }))}
      />
    </Form.Item>
  );
}

/**
 * Tag 选择器（tag_added 触发类型使用）。
 * 存储格式：{"tag_id": 3}
 */
function TagSelectorForm({ value, onChange }: {
  value: string;
  onChange: (json: string) => void;
}) {
  const [tags, setTags] = useState<Tag[]>([]);
  const [loading, setLoading] = useState(false);

  const parsed = useMemo(() => {
    try {
      const v = JSON.parse(value || '{}');
      return { tag_id: v.tag_id ?? null };
    } catch {
      return { tag_id: null };
    }
  }, [value]);

  const [selectedId, setSelectedId] = useState<number | null>(parsed.tag_id);

  // 加载所有 tag
  useEffect(() => {
    setLoading(true);
    db.getAllTags()
      .then((list) => setTags(list))
      .catch(() => { /* 静默 */ })
      .finally(() => setLoading(false));
  }, []);

  const handleChange = useCallback((id: number | null) => {
    setSelectedId(id);
    if (id) {
      onChange(JSON.stringify({ tag_id: id }));
    } else {
      onChange('{}');
    }
  }, [onChange]);

  return (
    <Form.Item label="选择标签" tooltip="当该标签被添加到任意 todo 时触发 Loop">
      <Select
        value={selectedId}
        onChange={handleChange}
        loading={loading}
        allowClear
        placeholder="选择一个标签"
        showSearch
        filterOption={(input, option) =>
          (option?.label ?? '').toLowerCase().includes(input.toLowerCase())
        }
        options={tags.map((t) => ({
          value: t.id,
          label: `${t.name}${t.color ? ` (${t.color})` : ''}`,
          color: t.color,
        }))}
        optionRender={(option) => (
          <span>
            {(option.data as any).color ? (
              <AntTag color={(option.data as any).color as string} style={{ marginRight: 6, lineHeight: '18px', fontSize: 11 }}>
                {option.data.label as string}
              </AntTag>
            ) : (
              option.data.label as string
            )}
          </span>
        )}
      />
    </Form.Item>
  );
}

/**
 * 飞书消息配置：选择 bot、chat_id、匹配方式和关键词。
 * 存储格式：{"bot_id":1,"chat_id":"oc_xxx","match_type":"contains","pattern":"hello"}
 */
function FeishuMessageConfigForm({ value, onChange }: {
  value: string;
  onChange: (json: string) => void;
}) {
  const [bots, setBots] = useState<{ id: number; name: string }[]>([]);
  const [loading, setLoading] = useState(false);

  const parsed = useMemo(() => {
    try {
      const v = JSON.parse(value || '{}');
      return {
        bot_id: v.bot_id ?? null,
        chat_id: v.chat_id ?? '',
        match_type: v.match_type ?? 'contains',
        pattern: v.pattern ?? '',
      };
    } catch {
      return { bot_id: null, chat_id: '', match_type: 'contains', pattern: '' };
    }
  }, [value]);

  const [botId, setBotId] = useState<number | null>(parsed.bot_id);
  const [chatId, setChatId] = useState(parsed.chat_id);
  const [matchType, setMatchType] = useState(parsed.match_type);
  const [pattern, setPattern] = useState(parsed.pattern);

  // 加载 bot 列表
  useEffect(() => {
    setLoading(true);
    db.getAgentBots()
      .then((list) => setBots(list.map((b: any) => ({ id: b.id, name: b.bot_name || b.bot_type }))))
      .catch(() => { /* 静默 */ })
      .finally(() => setLoading(false));
  }, []);

  useEffect(() => {
    const config: Record<string, any> = {};
    if (botId) config.bot_id = botId;
    if (chatId) config.chat_id = chatId;
    if (matchType) config.match_type = matchType;
    if (pattern) config.pattern = pattern;
    onChange(JSON.stringify(config));
  }, [botId, chatId, matchType, pattern, onChange]);

  return (
    <div>
      <Form.Item label="飞书机器人" tooltip="接收消息的飞书机器人">
        <Select
          value={botId}
          onChange={(v) => setBotId(v)}
          loading={loading}
          allowClear
          placeholder="选择机器人"
          options={bots.map((b) => ({ value: b.id, label: b.name }))}
        />
      </Form.Item>
      <Form.Item label="群聊/会话 ID" tooltip="留空则匹配任意群聊或会话（仅限已配置的机器人）">
        <Input
          value={chatId}
          onChange={(e) => setChatId(e.target.value)}
          placeholder="留空=任意会话，示例：oc_xxxxxx"
        />
      </Form.Item>
      <Form.Item label="匹配方式" tooltip="选择如何匹配消息内容">
        <Select
          value={matchType}
          onChange={(v) => setMatchType(v)}
          options={[
            { value: 'contains', label: '包含（contains）' },
            { value: 'exact', label: '精确匹配（exact）' },
            { value: 'regex', label: '正则匹配（regex）' },
          ]}
        />
      </Form.Item>
      <Form.Item label="关键词/正则" tooltip="匹配的消息内容关键词或正则表达式（留空=全部命中）">
        <Input
          value={pattern}
          onChange={(e) => setPattern(e.target.value)}
          placeholder={matchType === 'regex' ? '例如：^bug\\s+\\d+' : '输入关键词'}
        />
      </Form.Item>
    </div>
  );
}

/**
 * 飞书指令配置：选择 bot 和命令。
 * 存储格式：{"bot_id":1,"command":"/run"}
 */
function FeishuCommandConfigForm({ value, onChange }: {
  value: string;
  onChange: (json: string) => void;
}) {
  const [bots, setBots] = useState<{ id: number; name: string }[]>([]);
  const [loading, setLoading] = useState(false);

  const parsed = useMemo(() => {
    try {
      const v = JSON.parse(value || '{}');
      return { bot_id: v.bot_id ?? null, command: v.command ?? '' };
    } catch {
      return { bot_id: null, command: '' };
    }
  }, [value]);

  const [botId, setBotId] = useState<number | null>(parsed.bot_id);
  const [command, setCommand] = useState(parsed.command);

  useEffect(() => {
    setLoading(true);
    db.getAgentBots()
      .then((list) => setBots(list.map((b: any) => ({ id: b.id, name: b.bot_name || b.bot_type }))))
      .catch(() => { /* 静默 */ })
      .finally(() => setLoading(false));
  }, []);

  useEffect(() => {
    const config: Record<string, any> = {};
    if (botId) config.bot_id = botId;
    if (command) config.command = command;
    onChange(JSON.stringify(config));
  }, [botId, command, onChange]);

  return (
    <div>
      <Form.Item label="飞书机器人" tooltip="接收指令的飞书机器人">
        <Select
          value={botId}
          onChange={(v) => setBotId(v)}
          loading={loading}
          allowClear
          placeholder="选择机器人"
          options={bots.map((b) => ({ value: b.id, label: b.name }))}
        />
      </Form.Item>
      <Form.Item
        label="指令名称"
        tooltip="用户在飞书中发送的 slash 命令，例如 /run"
        rules={[{ required: false }]}
      >
        <Input
          value={command}
          onChange={(e) => setCommand(e.target.value)}
          placeholder="例如: /run, /review, /deploy"
        />
      </Form.Item>
    </div>
  );
}

/**
 * Todo 状态变更配置（todo_state_changed 专用）：选择 todo 和过滤条件。
 * 存储格式：{"todo_id":7,"to_status":"completed"}
 */
function TodoStateChangedConfigForm({ value, onChange }: {
  value: string;
  onChange: (json: string) => void;
}) {
  const [todos, setTodos] = useState<Todo[]>([]);
  const [loading, setLoading] = useState(false);

  const parsed = useMemo(() => {
    try {
      const v = JSON.parse(value || '{}');
      return {
        todo_id: v.todo_id ?? null,
        to_status: v.to_status ?? '',
      };
    } catch {
      return { todo_id: null, to_status: '' };
    }
  }, [value]);

  const [todoId, setTodoId] = useState<number | null>(parsed.todo_id);
  const [toStatus, setToStatus] = useState(parsed.to_status);

  useEffect(() => {
    setLoading(true);
    db.getAllTodos()
      .then((list) => setTodos(list))
      .catch(() => { /* 静默 */ })
      .finally(() => setLoading(false));
  }, []);

  useEffect(() => {
    const config: Record<string, any> = {};
    if (todoId) config.todo_id = todoId;
    if (toStatus) config.to_status = toStatus;
    onChange(JSON.stringify(config));
  }, [todoId, toStatus, onChange]);

  return (
    <div>
      <Form.Item label="选择 Todo" tooltip="该 Todo 的状态变化将触发 Loop（留空=任意 Todo 都触发）">
        <Select
          value={todoId}
          onChange={(v) => setTodoId(v)}
          loading={loading}
          allowClear
          placeholder="选择一个 todo（留空=任意）"
          showSearch
          filterOption={(input, option) =>
            (option?.label ?? '').toLowerCase().includes(input.toLowerCase())
          }
          options={todos.map((t) => ({
            value: t.id,
            label: t.title,
          }))}
        />
      </Form.Item>
      <Form.Item label="目标状态" tooltip="当 Todo 变更为该状态时触发（留空=任意状态变更均触发）">
        <Select
          value={toStatus}
          onChange={(v) => setToStatus(v)}
          allowClear
          placeholder="选择目标状态（留空=任意）"
          options={[
            { value: 'pending', label: 'pending（待执行）' },
            { value: 'running', label: 'running（执行中）' },
            { value: 'completed', label: 'completed（已完成）' },
            { value: 'failed', label: 'failed（失败）' },
            { value: 'cancelled', label: 'cancelled（已取消）' },
          ]}
        />
      </Form.Item>
    </div>
  );
}

// ====== 配置弹窗内容分发 ======

/**
 * 根据 trigger_type 返回对应的配置表单组件。
 * 对于 manual 这种无需配置的类型，返回 null。
 */
function TriggerConfigContent({ type, value, onChange }: {
  type: string;
  value: string;
  onChange: (json: string) => void;
}) {
  switch (type) {
    case 'cron':
      return <CronConfigForm value={value} onChange={onChange} />;
    case 'feishu_message':
      return <FeishuMessageConfigForm value={value} onChange={onChange} />;
    case 'feishu_command':
      return <FeishuCommandConfigForm value={value} onChange={onChange} />;
    case 'todo_completed':
      return <TodoSelectorForm value={value} onChange={onChange} />;
    case 'todo_state_changed':
      return <TodoStateChangedConfigForm value={value} onChange={onChange} />;
    case 'tag_added':
      return <TagSelectorForm value={value} onChange={onChange} />;
    default:
      // 兜底：未知类型，展示原始 JSON 编辑区
      return (
        <Form.Item label="配置 (JSON)" name="config">
          <Input.TextArea
            rows={5}
            placeholder="{}"
            style={{ fontFamily: 'monospace', fontSize: 12 }}
          />
        </Form.Item>
      );
  }
}

// ====== 主面板组件 ======

export function LoopTriggersPanel({ loopId, triggers, onChanged }: Props) {
  const { message } = AntApp.useApp();
  // 当前正在配置的 trigger_type (open=true 时): null=关闭
  const [configuring, setConfiguring] = useState<{ type: string; existing: LoopTriggerDto | null } | null>(null);
  const [saving, setSaving] = useState(false);
  // 各 trigger type 的临时配置 JSON 值（存放自定义配置表单的内容）
  const [configJson, setConfigJson] = useState('{}');
  const [form] = Form.useForm();

  // 按 type 分组索引已存在的 trigger（每种 type 最多 1 个）
  const byType = useMemo(() => {
    const m = new Map<string, LoopTriggerDto>();
    for (const t of triggers) m.set(t.trigger_type, t);
    return m;
  }, [triggers]);

  // 切换 enabled 状态（用户点 toggle 的非「新增」路径）
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
    setConfigJson('{}');
    form.resetFields();
  }, [form]);

  // 打开新增配置 modal（从 off 变 on）
  const openCreateConfig = useCallback((type: string) => {
    setConfigJson('{}');
    setConfiguring({ type, existing: null });
  }, []);

  // 打开编辑配置 modal（点击已存在的 trigger 行名）
  const openEditConfig = useCallback((t: LoopTriggerDto) => {
    setConfigJson(t.config);
    setConfiguring({ type: t.trigger_type, existing: t });
  }, []);

  // 提交配置（新建 / 更新）
  const handleSaveConfig = useCallback(async () => {
    if (!configuring) return;
    if (!configJson || configJson === 'undefined') {
      message.error('请完成配置后再保存');
      return;
    }
    setSaving(true);
    try {
      if (configuring.existing) {
        await dbLoops.updateTrigger(loopId, configuring.existing.id, {
          trigger_type: configuring.type,
          config: configJson || '{}',
          enabled: configuring.existing.enabled,
          priority: 0,
        } as UpdateTriggerRequest);
        message.success('已更新');
      } else {
        await dbLoops.createTrigger(loopId, {
          trigger_type: configuring.type,
          config: configJson || '{}',
          enabled: true,
          priority: 0,
        } as CreateTriggerRequest);
        message.success(`已添加「${TRIGGER_META[configuring.type]?.label ?? configuring.type}」触发器`);
      }
      closeConfig();
      onChanged();
    } catch {
      // ignore: 拦截器已弹错
    } finally {
      setSaving(false);
    }
  }, [configuring, configJson, loopId, message, closeConfig, onChanged]);

  // 删除已有 trigger
  const handleDelete = useCallback(async (id: number) => {
    try {
      await dbLoops.deleteTrigger(loopId, id);
      message.success('已删除');
      onChanged();
      if (configuring?.existing?.id === id) closeConfig();
    } catch {
      // ignore
    }
  }, [loopId, message, onChanged, configuring, closeConfig]);

  // 渲染一行 trigger
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
            style={{
              fontSize: 13, fontWeight: 500,
              cursor: existing ? 'pointer' : 'default',
              color: 'var(--color-text, #0f172a)',
            }}
            onClick={() => existing && openEditConfig(existing)}
          >
            {meta.label}
            {existing && (
              <EditOutlined style={{
                marginLeft: 6, fontSize: 11,
                color: 'var(--color-text-tertiary, #94a3b8)',
              }} />
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
          <Tooltip title={meta.noConfig ? '直接启用（无需配置）' : '配置后启用'}>
            <Button
              size="small"
              type="primary"
              onClick={() => {
                if (meta.noConfig) {
                  // manual 等无需配置的类型，直接启用（发送空配置）
                  dbLoops.createTrigger(loopId, {
                    trigger_type: type,
                    config: '{}',
                    enabled: true,
                    priority: 0,
                  } as CreateTriggerRequest).then(() => {
                    message.success(`已启用「${meta.label}」`);
                    onChanged();
                  }).catch(() => { /* 拦截器已弹错 */ });
                } else {
                  openCreateConfig(type);
                }
              }}
            >
              启用
            </Button>
          </Tooltip>
        )}
      </div>
    );
  };

  const allTypes = Object.keys(TRIGGER_META);

  return (
    <div className="loop-triggers-panel">
      <div style={{
        marginBottom: 12, fontSize: 12,
        color: 'var(--color-text-secondary, #475569)',
      }}>
        触发条件决定 loop 在何时被启动 · 当前已启用{' '}
        {triggers.filter(t => t.enabled).length} / 共 {allTypes.length} 条
      </div>

      {allTypes.map(renderRow)}

      {/* 配置 modal */}
      <Modal
        title={
          configuring?.existing
            ? `编辑「${TRIGGER_META[configuring.type]?.label ?? configuring.type}」`
            : `配置「${configuring ? (TRIGGER_META[configuring.type]?.label ?? '') : ''}」`
        }
        open={configuring !== null}
        onCancel={closeConfig}
        onOk={handleSaveConfig}
        okText={configuring?.existing ? '保存' : '启用'}
        cancelText="取消"
        confirmLoading={saving}
        destroyOnClose
        width={520}
        footer={configuring?.existing ? [
          <Button key="del" type="text" icon={<DeleteOutlined />} onClick={() => handleDelete(configuring.existing!.id)}>
            删除
          </Button>,
          <Button key="cancel" onClick={closeConfig}>取消</Button>,
          <Button key="ok" type="primary" loading={saving} onClick={handleSaveConfig}>保存</Button>,
        ] : undefined}
      >
        {configuring && (
          <>
            {/* 专用配置区域 */}
            <div style={{
              background: 'var(--color-bg-hover, #f1f5f9)',
              border: '1px solid var(--color-border, #e2e8f0)',
              borderRadius: 8,
              padding: 16,
            }}>
              <TriggerConfigContent
                type={configuring.type}
                value={configJson}
                onChange={setConfigJson}
              />
            </div>
          </>
        )}
      </Modal>
    </div>
  );
}
