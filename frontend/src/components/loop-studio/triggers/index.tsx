// Loop Studio 触发条件面板 - 主入口
//
// 设计目标：7 种触发类型（manual/cron/feishu_message/feishu_command/
// todo_completed/todo_state_changed/tag_added）全部以行内 toggle 展示。
// 每种类型有独立的配置弹窗，不再需要用户手写 JSON。
//
// 定时调度（cron）使用 react-js-cron + CronPresetSelect（和 todo 定时调度一致）。
// Todos / Tags / Feishu bots 等通过 Select 下拉选取已有数据。
//
// 子组件（CronConfigForm / TodoSelectorForm 等）仅在本目录内自用，
// 外部 caller 需要时直接 import 对应文件，不再 re-export。
// 例外：TRIGGER_META 是触发类型元数据（非 UI），LoopStudioDetailPanel 等
// 上层组件需要它做 label 渲染，作为目录对外 API 在此 re-export。

// 触发类型元数据：供 LoopStudioDetailPanel 渲染触发器标签时查询 label。
export { TRIGGER_META } from './helpers';

import { useState, useCallback, useMemo } from 'react';
import {
  App as AntApp,
  Button,
  Modal,
  Switch,
  Tooltip,
} from 'antd';
import {
  EditOutlined,
  DeleteOutlined,
} from '@ant-design/icons';
import * as dbLoops from '@/utils/database/loops';
import type { LoopTriggerDto, CreateTriggerRequest, UpdateTriggerRequest } from '@/types/loop';
import { formatRelativeTime } from '@/utils/datetime';
import { TRIGGER_META } from './helpers';
import { TriggerConfigContent } from './TriggerConfigContent';

interface Props {
  loopId: number;
  triggers: LoopTriggerDto[];
  onChanged: () => void;
  /** 按 loop 所属工作空间过滤可选的 todo（触发型触发器需要选同 workspace 的 todo） */
  workspaceId?: number | null;
}

// ====== 主面板组件 ======

export function LoopTriggersPanel({ loopId, triggers, onChanged, workspaceId }: Props) {
  const { message } = AntApp.useApp();
  // 当前正在配置的 trigger_type (open=true 时): null=关闭
  const [configuring, setConfiguring] = useState<{ type: string; existing: LoopTriggerDto | null } | null>(null);
  const [saving, setSaving] = useState(false);
  // 各 trigger type 的临时配置 JSON 值（存放自定义配置表单的内容）
  const [configJson, setConfigJson] = useState('{}');
  // 关闭配置 modal
  const closeConfig = useCallback(() => {
    setConfiguring(null);
    setConfigJson('{}');
  }, []);

  // 按 type 分组索引已存在的 trigger（每种 type 最多 1 个）
  const byType = useMemo(() => {
    const m = new Map<string, LoopTriggerDto>();
    for (const t of triggers) m.set(t.trigger_type, t);
    return m;
  }, [triggers]);

  // 切换 enabled 状态（用户点 toggle 的非「新增」路径）
  const handleToggleEnabled = useCallback(async (t: LoopTriggerDto, enabled: boolean) => {
    try {
      await dbLoops.updateTrigger(workspaceId ?? 0, loopId, t.id, {
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
        await dbLoops.updateTrigger(workspaceId ?? 0, loopId, configuring.existing.id, {
          trigger_type: configuring.type,
          config: configJson || '{}',
          enabled: configuring.existing.enabled,
          priority: 0,
        } as UpdateTriggerRequest);
        message.success('已更新');
      } else {
        await dbLoops.createTrigger(workspaceId ?? 0, loopId, {
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
      await dbLoops.deleteTrigger(workspaceId ?? 0, loopId, id);
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
                  dbLoops.createTrigger(workspaceId ?? 0, loopId, {
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
                workspaceId={workspaceId}
              />
            </div>
          </>
        )}
      </Modal>
    </div>
  );
}
