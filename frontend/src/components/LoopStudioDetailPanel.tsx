// Loop Studio 右栏 detail 容器。
//
// 对齐参考设计: 详情面板分成上下分段:
// - Header: 标题 + 操作 (触发/复制/启用暂停/编辑/删除)
// - 基本信息: 启用 Switch + 工作空间
// - 执行环节: 横向卡片列表（按顺序执行）, 最重要放在最前
// - 触发条件: 默认折叠, 仅展示已启用/共多少摘要
// - 执行历史: 折叠区, 默认收起 (不常用)
//
// (对齐 LoopDto 的可编辑字段)。

import { useEffect, useState, useCallback } from 'react';
import {
  Skeleton, App as AntApp, Button, Space, Tooltip, Popconfirm, Empty,
  Modal, Form, Input, InputNumber, ColorPicker, Collapse, Select, Switch,
} from 'antd';
import {
  ThunderboltOutlined,
  CopyOutlined,
  DeleteOutlined,
  EditOutlined,
  PlusOutlined,
} from '@ant-design/icons';
import * as dbLoops from '@/utils/database/loops';
import * as dbReviewTemplates from '@/utils/database/reviewTemplates';
import * as db from '@/utils/database';
import type { LoopDetail, UpdateLoopRequest } from '@/types/loop';
import type { ReviewTemplateOption } from '@/types/reviewTemplate';
import type { ProjectDirectory } from '@/types';
import { LoopTriggersPanel, TRIGGER_META } from './LoopStudioTriggersPanel';
import { LoopStepsPanel } from './LoopStudioStepsPanel';
import { LoopExecutionsPanel } from './LoopStudioExecutionsPanel';

interface LoopDetailPanelProps {
  loopId: number;
  onTrigger: () => void;
  onDuplicate: () => void;
  onDelete: () => void;
  onCreate: () => void;
  onToggleStatus: () => void;
  onChanged: () => void;
}

export function LoopDetailPanel({
  loopId,
  onTrigger,
  onDuplicate,
  onDelete,
  onCreate,
  onToggleStatus,
  onChanged,
}: LoopDetailPanelProps) {
  const { message } = AntApp.useApp();
  const [detail, setDetail] = useState<LoopDetail | null>(null);
  const [loading, setLoading] = useState(true);
  // 基础信息编辑 modal 开关 (替代之前的 inline 编辑)
  const [editing, setEditing] = useState(false);
  const [saving, setSaving] = useState(false);
  const [form] = Form.useForm<UpdateLoopRequest & { max_step_executions?: number; max_total_tokens?: number }>();
  // 工作空间下拉选项
  const [workspaceOptions, setWorkspaceOptions] = useState<{ label: string; value: string }[]>([]);
  // 完整的项目目录列表（用于展示详情）
  const [projectDirs, setProjectDirs] = useState<ProjectDirectory[]>([]);
  // 执行记录总数，由 LoopExecutionsPanel 通过回调更新
  const [executionTotal, setExecutionTotal] = useState(0);

  // 加载完整 detail, 子面板变更后也要重新拉以保持最新
  const reload = useCallback(() => {
    setLoading(true);
    dbLoops.getLoop(loopId)
      .then((d) => {
        setDetail(d);
        form.setFieldsValue({
          name: d.name,
          description: d.description,
          workspace: d.workspace,
          color: d.color,
          icon: d.icon,
        });
        // 解析 limits_config 到同一 form
        try {
          const lc = JSON.parse(d.limits_config || '{}');
          form.setFieldsValue({
            max_step_executions: lc.max_step_executions ?? null,
            max_total_tokens: lc.max_total_tokens ?? null,
          });
        } catch {
          // 忽略解析错误
        }
      })
      .catch(() => {
        message.error('加载 loop 详情失败');
        setDetail(null);
      })
      .finally(() => setLoading(false));
  }, [loopId, form, message]);

  useEffect(() => { reload(); }, [reload]);

  // 加载工作空间列表供下拉选择
  useEffect(() => {
    db.getProjectDirectories()
      .then(dirs => {
        setProjectDirs(dirs);
        setWorkspaceOptions(
          dirs.map(d => ({ label: d.name || d.path, value: d.path }))
        );
      })
      .catch(() => { /* 静默 */ });
  }, []);

  // 评审模板下拉选项 (含 inline 「+ 新建模板」流程所需)
  const [reviewTemplateOptions, setReviewTemplateOptions] = useState<ReviewTemplateOption[]>([]);
  const [creatingTemplate, setCreatingTemplate] = useState(false);
  const [newTemplateForm] = Form.useForm<{ name: string; description?: string; prompt: string }>();

  // 加载评审模板选项；保存/创建后也要重新拉以保持最新
  const reloadTemplateOptions = useCallback(() => {
    dbReviewTemplates.listReviewTemplateOptions()
      .then(setReviewTemplateOptions)
      .catch(() => { /* 静默：模板加载失败不影响 loop 编辑 */ });
  }, []);

  useEffect(() => { reloadTemplateOptions(); }, [reloadTemplateOptions]);

  /**
   * Inline 创建评审模板：弹一个小 Modal，输入 name + prompt，保存后
   * 1) 把新模板写回 reviewTemplateOptions  2) 把 Select 当前值置为新 id
   * 避免用户先关掉 loop 编辑器去设置里建模板再回来。
   */
  const handleCreateTemplate = useCallback(async () => {
    const values = await newTemplateForm.validateFields();
    setCreatingTemplate(true);
    try {
      const created = await dbReviewTemplates.createReviewTemplate({
        name: values.name.trim(),
        description: values.description?.trim() || null,
        prompt: values.prompt,
      });
      message.success(`已创建模板「${created.name}」`);
      // 刷新下拉 + 立即选中新建的
      const opts = await dbReviewTemplates.listReviewTemplateOptions();
      setReviewTemplateOptions(opts);
      form.setFieldsValue({ review_template_id: created.id });
      newTemplateForm.resetFields();
      setCreatingTemplate(false);
    } catch (e) {
      message.error(`创建失败：${(e as Error).message}`);
    }
  }, [form, message, newTemplateForm]);

  // 预加载执行记录总数（用于折叠标签展示，不等用户展开后才显示）
  useEffect(() => {
    dbLoops.listExecutions(loopId, { page: 1, limit: 1 })
      .then(res => setExecutionTotal(res.total))
      .catch(() => { /* 静默 */ });
  }, [loopId]);

  // 打开编辑 modal
  const handleOpenEdit = useCallback(() => {
    if (!detail) return;
    form.setFieldsValue({
      name: detail.name,
      description: detail.description,
      workspace: detail.workspace,
      color: detail.color,
      icon: detail.icon,
      // review_template_id 是 Option<i64>，null 也要能透传
      review_template_id: detail.review_template_id ?? null,
    });
    setEditing(true);
  }, [detail, form]);

  // 保存基础信息 (后端要求全量)
  const handleSave = useCallback(async () => {
    const values = await form.validateFields();
    setSaving(true);
    try {
      const colorHex = String(values.color || 'var(--color-primary, #0891b2)');
      // 构建 limits_config（从主 form 读取）
      const limitsConfig: Record<string, any> = {};
      if (values.max_step_executions != null) limitsConfig.max_step_executions = values.max_step_executions;
      if (values.max_total_tokens != null) limitsConfig.max_total_tokens = values.max_total_tokens;

      await dbLoops.updateLoop(loopId, {
        name: values.name.trim(),
        description: values.description ?? '',
        workspace: values.workspace ?? null,
        color: colorHex,
        icon: values.icon ?? 'loop',
        review_template_id: values.review_template_id ?? null,
        limits_config: Object.keys(limitsConfig).length > 0 ? JSON.stringify(limitsConfig) : null,
      });
      message.success('已保存');
      setEditing(false);
      reload();
      onChanged();
    } catch (e) {
      message.error('保存失败，请重试');
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
          <Button size="small" icon={<EditOutlined />} onClick={handleOpenEdit}>编辑</Button>
          <Button size="small" icon={<PlusOutlined />} onClick={onCreate}>新建</Button>
          <Popconfirm
            title="删除 loop"
            description="将级联删除 triggers/steps,无法恢复"
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
          <DetailField label="启用状态" value={
            <span style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
              <Switch
                checked={detail.status === 'enabled'}
                onChange={() => {
                  onToggleStatus();
                  // 切换后立即刷新详情, 让 Switch 和状态文字同步更新
                  setTimeout(() => { reload(); onChanged(); }, 100);
                }}
              />
              <span style={{
                fontSize: 12, fontWeight: 500,
                color: detail.status === 'enabled'
                  ? 'var(--color-success, #22c55e)'
                  : detail.status === 'paused'
                    ? 'var(--color-warning, #f59e0b)'
                    : 'var(--color-text-tertiary, #94a3b8)',
              }}>
                {detail.status === 'enabled' ? '已启用' : detail.status === 'paused' ? '已暂停' : '草稿'}
              </span>
            </span>
          } />
          <DetailField label="关联工作空间" value={
            detail.workspace ? (() => {
              const dir = projectDirs.find(d => d.path === detail.workspace);
              return dir ? (
                <div>
                  <div style={{ fontWeight: 500 }}>{dir.name || dir.path}</div>
                  <div style={{ fontSize: 11, color: 'var(--color-text-tertiary, #94a3b8)', marginTop: 2 }}>
                    {dir.path}
                    {dir.git_worktree_enabled && (
                      <span style={{ marginLeft: 8 }}>· Git Worktree</span>
                    )}
                    {dir.auto_cleanup && (
                      <span style={{ marginLeft: 4 }}>· 自动清理</span>
                    )}
                  </div>
                </div>
              ) : (
                <span>{detail.workspace}</span>
              );
            })() : <EmptyValue />
          } />
        </div>
      </DetailSection>

      {/* 全局限制 */}
      <DetailSection title="全局限制">
        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(3, 1fr)', gap: 16 }}>
          <DetailField label="最大执行步数" value={
            (() => {
              try {
                const lc = JSON.parse(detail.limits_config || '{}');
                return lc.max_step_executions != null
                  ? <span style={{ fontWeight: 500 }}>{lc.max_step_executions} 步</span>
                  : <span style={{ color: '#94a3b8' }}>未限制</span>;
              } catch { return <EmptyValue />; }
            })()
          } />
          <DetailField label="最大 Token 数" value={
            (() => {
              try {
                const lc = JSON.parse(detail.limits_config || '{}');
                return lc.max_total_tokens != null
                  ? <span style={{ fontWeight: 500 }}>{lc.max_total_tokens.toLocaleString()}</span>
                  : <span style={{ color: '#94a3b8' }}>未限制</span>;
              } catch { return <EmptyValue />; }
            })()
          } />
        </div>
      </DetailSection>

      {/* 执行环节: DAG 流程图 */}
      <DetailSection title="执行环节" extra={
        <span style={{ fontSize: 11, color: 'var(--color-text-tertiary, #94a3b8)' }}>
          {detail.steps.length} 个环节按顺序执行
        </span>
      }>
        <LoopStepsPanel
          loopId={loopId}
          steps={detail.steps}
          onChanged={() => { reload(); onChanged(); }}
        />
      </DetailSection>

      {/* 触发条件: 默认折叠, 仅展示摘要计数 */}
      <div style={{
        background: 'var(--color-bg-elevated, #ffffff)',
        border: '1px solid var(--color-border, #e2e8f0)',
        borderRadius: 8,
        marginBottom: 12,
        overflow: 'hidden',
      }}>
        <Collapse
          ghost
          expandIconPosition="end"
          defaultActiveKey={[]}
          items={[
            {
              key: 'triggers',
              label: (
                <span style={{ fontSize: 14, fontWeight: 600, color: 'var(--color-text, #0f172a)' }}>
                  触发条件
                  <span style={{ fontSize: 11, fontWeight: 400, color: 'var(--color-text-tertiary, #94a3b8)', marginLeft: 8 }}>
                    {detail.triggers.filter(t => t.enabled).length} / {Object.keys(TRIGGER_META).length} 已启用
                  </span>
                </span>
              ),
              children: (
                <div style={{ paddingTop: 4 }}>
                  <LoopTriggersPanel
                    loopId={loopId}
                    triggers={detail.triggers}
                    onChanged={() => { reload(); onChanged(); }}
                  />
                </div>
              ),
            },
          ]}
        />
      </div>

      {/* 折叠区: 执行历史, 默认收起 */}
      <div style={{
        background: 'var(--color-bg-elevated, #ffffff)',
        border: '1px solid var(--color-border, #e2e8f0)',
        borderRadius: 8,
        marginTop: 12,
        overflow: 'hidden',
      }}>
        <Collapse
          ghost
          expandIconPosition="end"
          defaultActiveKey={[]}
          items={[
            {
              key: 'executions',
              label: (
                <span style={{ fontSize: 14, fontWeight: 600, color: 'var(--color-text, #0f172a)' }}>
                  执行历史
                  {executionTotal > 0 && (
                    <span style={{ fontSize: 11, fontWeight: 400, color: 'var(--color-text-tertiary, #94a3b8)', marginLeft: 8 }}>
                      共 {executionTotal} 条
                    </span>
                  )}
                </span>
              ),
              children: (
                <div style={{ paddingTop: 4 }}>
                  <LoopExecutionsPanel loopId={loopId} loopName={detail.name} onTotalChange={setExecutionTotal} />
                </div>
              ),
            },
          ]}
        />
      </div>

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
          <Form.Item label="关联工作空间" name="workspace" tooltip="此 loop 所属的工作空间">
            <Select
              allowClear
              placeholder="选择工作空间"
              options={workspaceOptions}
              showSearch
              optionFilterProp="label"
            />
          </Form.Item>
          <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 12 }}>
            <Form.Item label="颜色" name="color" getValueFromEvent={(c) => c?.toHexString?.() ?? c}>
              <ColorPicker showText format="hex" />
            </Form.Item>
            <Form.Item label="图标" name="icon" tooltip="预留字段, 当前仅展示">
              <Input placeholder="loop" maxLength={50} />
            </Form.Item>
          </div>
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
              // 不传 options 会被 antd 当成"自定义模板字符串"模式（与 value 是 number 冲突）。
              options={reviewTemplateOptions.map((t) => ({
                value: t.id,
                label: t.name,
              }))}
            />
          </Form.Item>
          {/* ── 全局限制 ── */}
          <div style={{ fontWeight: 600, fontSize: 14, marginTop: 16, marginBottom: 12, color: 'var(--color-text-secondary, #64748b)' }}>
            全局限制
          </div>
          <div style={{ background: 'var(--color-bg-elevated, #f8fafc)', padding: 12, borderRadius: 8 }}>
            <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 12 }}>
              <Form.Item label="最大执行步数" name={['max_step_executions']} tooltip="超出后自动终止 Loop（留空=不限制）">
                <InputNumber min={1} max={9999} placeholder="不限" style={{ width: '100%' }} />
              </Form.Item>
              <Form.Item label="最大 Token 数（预留）" name={['max_total_tokens']} tooltip="超出后自动终止（留空=不限制）">
                <InputNumber min={1} max={9999999999} placeholder="不限" style={{ width: '100%' }} step={1000000} />
              </Form.Item>
            </div>
          </div>
        </Form>
      </Modal>

      {/* Inline 新建评审模板的 Modal：在 loop 编辑器内就能创建并立即选中。 */}
      <Modal
        title="新建评审模板"
        open={creatingTemplate}
        onCancel={() => {
          newTemplateForm.resetFields();
          setCreatingTemplate(false);
        }}
        onOk={handleCreateTemplate}
        confirmLoading={creatingTemplate}
        destroyOnClose
      >
        <Form form={newTemplateForm} layout="vertical" preserve={false}>
          <Form.Item
            label="名称"
            name="name"
            rules={[{ required: true, message: '请输入名称' }]}
          >
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
            <Input.TextArea
              rows={8}
              placeholder="你是一个评审师…"
            />
          </Form.Item>
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