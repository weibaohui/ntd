// Loop Studio 右栏 detail 容器。
//
// 对齐参考设计: 详情面板分成上下分段:
// - Header: 标题 + 操作 (触发/复制/启用暂停/编辑/删除)
// - 基本信息: 3 列 (状态 / 产品 / 仓库分支)
// - 触发条件: 内联 toggle 列表 (8 种类型一行一条)
// - 执行环节: 关联的 todo 列表（按顺序执行）
// - 钩子 / 执行历史: 折叠区, 默认收起 (不常用)
//
// 编辑入口从 inline 改成完整 modal, 含 product / repo / branch 字段
// (对齐 LoopDto 的 7 个可编辑字段)。

import { useEffect, useState, useCallback } from 'react';
import {
  Skeleton, App as AntApp, Button, Space, Tooltip, Popconfirm, Empty,
  Modal, Form, Input, ColorPicker, Collapse,
} from 'antd';
import {
  ThunderboltOutlined,
  CopyOutlined,
  DeleteOutlined,
  PlayCircleOutlined,
  PauseCircleOutlined,
  EditOutlined,
} from '@ant-design/icons';
import * as dbLoops from '@/utils/database/loops';
import type { LoopDetail, UpdateLoopRequest } from '@/types/loop';
import { LoopTriggersPanel } from './LoopStudioTriggersPanel';
import { LoopHooksPanel } from './LoopStudioHooksPanel';
import { LoopExecutionsPanel } from './LoopStudioExecutionsPanel';

interface LoopDetailPanelProps {
  loopId: number;
  onTrigger: () => void;
  onDuplicate: () => void;
  onDelete: () => void;
  onToggleStatus: () => void;
  onChanged: () => void;
}

export function LoopDetailPanel({
  loopId,
  onTrigger,
  onDuplicate,
  onDelete,
  onToggleStatus,
  onChanged,
}: LoopDetailPanelProps) {
  const { message } = AntApp.useApp();
  const [detail, setDetail] = useState<LoopDetail | null>(null);
  const [loading, setLoading] = useState(true);
  // 基础信息编辑 modal 开关 (替代之前的 inline 编辑)
  const [editing, setEditing] = useState(false);
  const [saving, setSaving] = useState(false);
  const [form] = Form.useForm<UpdateLoopRequest>();

  // 加载完整 detail, 子面板变更后也要重新拉以保持最新
  const reload = useCallback(() => {
    setLoading(true);
    dbLoops.getLoop(loopId)
      .then((d) => {
        setDetail(d);
        form.setFieldsValue({
          name: d.name,
          description: d.description,
          product: d.product,
          repo: d.repo,
          branch: d.branch,
          color: d.color,
          icon: d.icon,
        });
      })
      .catch(() => {
        message.error('加载 loop 详情失败');
        setDetail(null);
      })
      .finally(() => setLoading(false));
  }, [loopId, form, message]);

  useEffect(() => { reload(); }, [reload]);

  // 打开编辑 modal
  const handleOpenEdit = useCallback(() => {
    if (!detail) return;
    form.setFieldsValue({
      name: detail.name,
      description: detail.description,
      product: detail.product,
      repo: detail.repo,
      branch: detail.branch,
      color: detail.color,
      icon: detail.icon,
    });
    setEditing(true);
  }, [detail, form]);

  // 保存基础信息 (后端要求全量)
  const handleSave = useCallback(async () => {
    const values = await form.validateFields();
    setSaving(true);
    try {
      const colorHex = String(values.color || 'var(--color-primary, #0891b2)');
      await dbLoops.updateLoop(loopId, {
        name: values.name.trim(),
        description: values.description ?? '',
        product: values.product ?? '',
        repo: values.repo ?? '',
        branch: values.branch ?? '',
        color: colorHex,
        icon: values.icon ?? 'loop',
      });
      message.success('已保存');
      setEditing(false);
      reload();
      onChanged();
    } catch {
      // ignore: 表单错误会显示
    } finally {
      setSaving(false);
    }
  }, [form, loopId, message, reload, onChanged]);

  if (loading && !detail) {
    return <Skeleton active style={{ padding: 24 }} />;
  }
  if (!detail) {
    return <Empty description="无法加载该 loop" style={{ marginTop: 64 }} />;
  }

  return (
    // 父容器已 overflow:auto, 这里只负责垂直 padding, 不再 height:100%
    <div className="loop-detail-panel" style={{ padding: '20px 24px' }}>
      {/* Header: 颜色条 + 标题 + 操作按钮 */}
      <div className="loop-detail-header" style={{ display: 'flex', alignItems: 'center', gap: 12, marginBottom: 16 }}>
        <span style={{ width: 4, height: 24, background: detail.color, borderRadius: 2 }} />
        <h2 style={{ margin: 0, fontSize: 18, flex: 1, minWidth: 0, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap', color: 'var(--color-text, #0f172a)' }}>
          {detail.name}
        </h2>
        <span style={{ color: 'var(--color-text-tertiary, #94a3b8)', fontSize: 12 }}>#{detail.id}</span>
        <Space size={4}>
          <Tooltip title="手动触发">
            <Button
              size="small" type="primary"
              icon={<ThunderboltOutlined />}
              onClick={onTrigger}
              disabled={detail.status !== 'enabled'}
            >
              触发
            </Button>
          </Tooltip>
          <Tooltip title="复制">
            <Button size="small" icon={<CopyOutlined />} onClick={onDuplicate} />
          </Tooltip>
          <Tooltip title={detail.status === 'enabled' ? '暂停' : '启用'}>
            <Button
              size="small"
              icon={detail.status === 'enabled' ? <PauseCircleOutlined /> : <PlayCircleOutlined />}
              onClick={onToggleStatus}
            />
          </Tooltip>
          <Button size="small" icon={<EditOutlined />} onClick={handleOpenEdit}>编辑</Button>
          <Popconfirm
            title="删除 loop"
            description="将级联删除 triggers/stages/hooks,无法恢复"
            okType="danger"
            onConfirm={onDelete}
          >
            <Button size="small" danger icon={<DeleteOutlined />} />
          </Popconfirm>
        </Space>
      </div>

      {detail.description && (
        <div style={{ color: 'var(--color-text-secondary, #475569)', fontSize: 13, marginBottom: 16 }}>{detail.description}</div>
      )}

      {/* Section: 基本信息 — 3 列布局, 与参考设计一致 */}
      <DetailSection title="基本信息">
        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(3, 1fr)', gap: 16 }}>
          <DetailField label="状态" value={
            <span style={{
              display: 'inline-block', padding: '2px 10px', borderRadius: 10,
              background: detail.status === 'enabled'
                ? 'var(--color-success-bg, #f0fdf4)'
                : detail.status === 'paused'
                  ? 'var(--color-warning-bg, #fffbeb)'
                  : 'var(--color-bg-hover, #f1f5f9)',
              color: detail.status === 'enabled'
                ? 'var(--color-success, #22c55e)'
                : detail.status === 'paused'
                  ? 'var(--color-warning, #f59e0b)'
                  : 'var(--color-text-tertiary, #94a3b8)',
              fontSize: 12, fontWeight: 500,
            }}>
              {detail.status === 'enabled' ? '已启用' : detail.status === 'paused' ? '已暂停' : '草稿'}
            </span>
          } />
          <DetailField label="产品" value={detail.product || <EmptyValue />} />
          <DetailField label="仓库 / 分支" value={
            detail.repo || detail.branch
              ? <span style={{ fontFamily: 'monospace', fontSize: 12 }}>{detail.repo || '?'}{detail.repo && detail.branch ? ' · ' : ''}<span style={{ color: 'var(--color-primary, #0891b2)' }}>{detail.branch || ''}</span></span>
              : <EmptyValue />
          } />
        </div>
      </DetailSection>

      {/* Section: 触发条件 — 内联 toggle 列表 */}
      <DetailSection title="触发条件" extra={
        <span style={{ fontSize: 11, color: 'var(--color-text-tertiary, #94a3b8)' }}>
          决定 loop 在何时被启动 · {detail.triggers.filter(t => t.enabled).length} / {detail.triggers.length} 已启用
        </span>
      }>
        <LoopTriggersPanel
          loopId={loopId}
          triggers={detail.triggers}
          onChanged={() => { reload(); onChanged(); }}
        />
      </DetailSection>

      {/* 执行环节: 关联的 todo 列表 */}
      <DetailSection title="执行环节" extra={
        <span style={{ fontSize: 11, color: 'var(--color-text-tertiary, #94a3b8)' }}>
          {detail.stages.length} 个环节按顺序执行
        </span>
      }>
        <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
          {detail.stages.map((s, idx) => {
            const todo = detail.todo_map[s.todo_id];
            return (
              <div key={s.id} style={{
                display: 'flex', alignItems: 'center', gap: 8,
                padding: '8px 12px',
                background: 'var(--color-bg-elevated, #fff)',
                border: '1px solid var(--color-border, #e2e8f0)',
                borderRadius: 8,
              }}>
                <span style={{
                  width: 24, height: 24, borderRadius: 12,
                  background: 'var(--color-primary, #0891b2)', color: '#fff',
                  display: 'flex', alignItems: 'center', justifyContent: 'center',
                  fontSize: 11, fontWeight: 600, flexShrink: 0,
                }}>{idx + 1}</span>
                <div style={{ flex: 1, minWidth: 0 }}>
                  <div style={{ fontWeight: 500, fontSize: 13, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                    {todo ? `#${todo.id} ${todo.title}` : `todo #${s.todo_id}`}
                  </div>
                  {s.description && (
                    <div style={{ fontSize: 11, color: 'var(--color-text-tertiary, #94a3b8)' }}>{s.description}</div>
                  )}
                </div>
                <span style={{ fontSize: 11, color: 'var(--color-text-tertiary, #94a3b8)' }}>
                  {s.enabled ? '已启用' : '已禁用'}
                </span>
              </div>
            );
          })}
        </div>
      </DetailSection>

      {/* 折叠区: 钩子 + 执行历史, 默认收起 (不常用, 避免首屏信息过载) */}
      <Collapse
        ghost
        style={{ marginTop: 8 }}
        items={[
          {
            key: 'hooks',
            label: <span style={{ fontSize: 14, fontWeight: 600 }}>钩子 ({detail.hooks.length})</span>,
            children: (
              <LoopHooksPanel
                loopId={loopId}
                hooks={detail.hooks}
                stages={detail.stages}
                todoMap={detail.todo_map}
                onChanged={() => { reload(); onChanged(); }}
              />
            ),
          },
          {
            key: 'executions',
            label: <span style={{ fontSize: 14, fontWeight: 600 }}>执行历史</span>,
            children: (
              <LoopExecutionsPanel loopId={loopId} loopName={detail.name} />
            ),
          },
        ]}
      />

      {/* 编辑基础信息 modal — 替代之前的 inline 编辑 */}
      <Modal
        title="编辑 loop"
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
          <Form.Item label="名称" name="name" rules={[{ required: true, message: '名称必填' }]}>
            <Input maxLength={100} />
          </Form.Item>
          <Form.Item label="描述" name="description">
            <Input.TextArea rows={2} maxLength={500} />
          </Form.Item>
          <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 12 }}>
            <Form.Item label="产品" name="product" tooltip="用于在 loop 列表与详情里区分不同 loop 的归属">
              <Input placeholder="例如:电商中台" maxLength={100} />
            </Form.Item>
            <Form.Item label="仓库" name="repo" tooltip="关联的 git 仓库, 展示用, 不做 clone">
              <Input placeholder="例如:order-svc.git" maxLength={200} />
            </Form.Item>
          </div>
          <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr 1fr', gap: 12 }}>
            <Form.Item label="分支" name="branch">
              <Input placeholder="例如:main" maxLength={100} />
            </Form.Item>
            <Form.Item label="颜色" name="color" getValueFromEvent={(c) => c?.toHexString?.() ?? c}>
              <ColorPicker showText format="hex" />
            </Form.Item>
            <Form.Item label="图标" name="icon" tooltip="预留字段, 当前仅展示">
              <Input placeholder="loop" maxLength={50} />
            </Form.Item>
          </div>
        </Form>
      </Modal>
    </div>
  );
}

// 段标题 + 卡片容器
function DetailSection({ title, extra, children }: {
  title: string;
  extra?: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <section style={{
      background: 'var(--color-bg-elevated, #ffffff)',
      border: '1px solid var(--color-border, #e2e8f0)',
      borderRadius: 8,
      padding: 16,
      marginBottom: 12,
    }}>
      <div style={{
        display: 'flex', alignItems: 'center', gap: 8,
        marginBottom: 12,
        fontSize: 14, fontWeight: 600,
        color: 'var(--color-text, #0f172a)',
      }}>
        <span style={{ flex: 1 }}>{title}</span>
        {extra}
      </div>
      {children}
    </section>
  );
}

// 基本信息的一个字段 (label + value, 2 行)
function DetailField({ label, value }: { label: string; value: React.ReactNode }) {
  return (
    <div>
      <div style={{ fontSize: 11, color: 'var(--color-text-tertiary, #94a3b8)', marginBottom: 4 }}>{label}</div>
      <div style={{ fontSize: 13, color: 'var(--color-text, #0f172a)' }}>{value}</div>
    </div>
  );
}

// 空值占位
function EmptyValue() {
  return <span style={{ color: 'var(--color-text-tertiary, #94a3b8)' }}>—</span>;
}