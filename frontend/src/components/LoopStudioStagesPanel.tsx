// Loop Studio 执行环节面板：横向卡片布局，支持增删改排。
//
// 设计参考：流水线阶段卡片式 UI
// - 每张卡片展示环节序号、名称、描述、执行者
// - 卡片间以箭头连接表示顺序
// - 最右侧虚线「添加环节」按钮
// - 支持拖拽重排、启用/禁用切换、删除

import { useState, useRef, useCallback } from 'react';
import {
  App as AntApp, Button, Modal, Form, Input, Select, Switch, Popconfirm, Empty,
} from 'antd';
import {
  PlusOutlined,
  ArrowRightOutlined,
  DeleteOutlined,
  DragOutlined,
} from '@ant-design/icons';
import * as dbLoops from '@/utils/database/loops';
import * as dbSteps from '@/utils/database/steps';
import type { LoopStageDto, CreateStageRequest } from '@/types/loop';
import type { Todo } from '@/types';

interface StagesPanelProps {
  loopId: number;
  stages: LoopStageDto[];
  onChanged: () => void;
}

export function LoopStagesPanel({ loopId, stages, onChanged }: StagesPanelProps) {
  const { message } = AntApp.useApp();

  // Modal 状态
  const [modalOpen, setModalOpen] = useState(false);
  const [editingStage, setEditingStage] = useState<LoopStageDto | null>(null);
  const [saving, setSaving] = useState(false);
  const [candidates, setCandidates] = useState<Todo[]>([]);
  const [form] = Form.useForm<CreateStageRequest & { todo_title: string }>();

  // Hover 状态（显示删除按钮）
  const [hoveredStageId, setHoveredStageId] = useState<number | null>(null);

  // 拖拽状态
  const [dragIndex, setDragIndex] = useState<number | null>(null);
  const dragNode = useRef<HTMLElement | null>(null);

  // 打开新增 Modal
  const handleOpenAdd = useCallback(async () => {
    setEditingStage(null);
    form.resetFields();
    try {
      const list = await dbSteps.listStepCandidates();
      setCandidates(list);
    } catch {
      setCandidates([]);
    }
    setModalOpen(true);
  }, [form]);

  // 打开编辑 Modal
  const handleOpenEdit = useCallback(async (stage: LoopStageDto) => {
    setEditingStage(stage);
    try {
      const list = await dbSteps.listStepCandidates();
      setCandidates(list);
    } catch {
      setCandidates([]);
    }
    form.setFieldsValue({
      name: stage.name,
      todo_id: stage.todo_id,
      description: stage.description,
      enabled: stage.enabled,
    });
    setModalOpen(true);
  }, [form]);

  // 保存（新增或更新）
  const handleSave = useCallback(async () => {
    const values = await form.validateFields();
    setSaving(true);
    try {
      if (editingStage) {
        await dbLoops.updateStage(loopId, editingStage.id, {
          name: values.name.trim(),
          description: values.description ?? '',
          todo_id: values.todo_id,
          run_mode: 'sequential',
          skip_on_source_failed: false,
          min_rating: null,
          unrated_policy: 'skip',
          enabled: values.enabled ?? true,
        });
        message.success('环节已更新');
      } else {
        await dbLoops.createStage(loopId, {
          name: values.name.trim(),
          description: values.description ?? '',
          todo_id: values.todo_id,
          run_mode: 'sequential',
          skip_on_source_failed: false,
          min_rating: null,
          unrated_policy: 'skip',
          enabled: values.enabled ?? true,
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
  }, [form, loopId, editingStage, message, onChanged]);

  // 删除环节
  const handleDelete = useCallback(async (stageId: number) => {
    try {
      await dbLoops.deleteStage(loopId, stageId);
      message.success('环节已删除');
      onChanged();
    } catch {
      // 后端错误已有 message
    }
  }, [loopId, message, onChanged]);

  // 拖拽事件
  const handleDragStart = useCallback((e: React.DragEvent<HTMLDivElement>, index: number) => {
    dragNode.current = e.target as HTMLElement;
    setDragIndex(index);
    e.dataTransfer.effectAllowed = 'move';
    setTimeout(() => {
      if (dragNode.current) {
        dragNode.current.style.opacity = '0.4';
      }
    }, 0);
  }, []);

  const handleDragOver = useCallback((e: React.DragEvent<HTMLDivElement>, index: number) => {
    e.preventDefault();
    e.dataTransfer.dropEffect = 'move';
    if (dragIndex === null || dragIndex === index) return;
    setDragIndex(index);
  }, [dragIndex]);

  const handleDragEnd = useCallback(async () => {
    if (dragNode.current) {
      dragNode.current.style.opacity = '1';
    }
    if (dragIndex !== null) {
      setDragIndex(null);
      const orderedIds = stages.map(s => s.id);
      try {
        await dbLoops.reorderStages(loopId, orderedIds);
        onChanged();
      } catch {
        // 静默
      }
    }
  }, [dragIndex, stages, loopId, onChanged]);

  // 选择 todo 后自动填充 name
  const handleTodoChange = useCallback((todo_id: number) => {
    const selected = candidates.find(c => c.id === todo_id);
    if (selected) {
      const currentName = form.getFieldValue('name');
      if (!currentName || !editingStage) {
        form.setFieldsValue({ name: selected.title, todo_title: selected.title });
      }
    }
  }, [candidates, form, editingStage]);

  return (
    <>
      <div style={{ display: 'flex', gap: 0, overflowX: 'auto', paddingBottom: 8, alignItems: 'stretch' }}>
        {stages.length === 0 ? (
          <div
            onClick={handleOpenAdd}
            role="button"
            tabIndex={0}
            onKeyDown={(e) => { if (e.key === 'Enter') handleOpenAdd(); }}
            style={{
              display: 'flex', flexDirection: 'column', alignItems: 'center', justifyContent: 'center',
              minWidth: 200, minHeight: 160, width: '100%',
              border: '2px dashed var(--color-border, #e2e8f0)',
              borderRadius: 12, cursor: 'pointer',
              color: 'var(--color-text-tertiary, #94a3b8)',
              fontSize: 13, gap: 8,
              transition: 'border-color 200ms, color 200ms',
            }}
            onMouseEnter={(e) => { e.currentTarget.style.borderColor = 'var(--color-primary, #0891b2)'; e.currentTarget.style.color = 'var(--color-primary, #0891b2)'; }}
            onMouseLeave={(e) => { e.currentTarget.style.borderColor = 'var(--color-border, #e2e8f0)'; e.currentTarget.style.color = 'var(--color-text-tertiary, #94a3b8)'; }}
          >
            <PlusOutlined style={{ fontSize: 24 }} />
            <span>暂无执行环节，点击添加</span>
          </div>
        ) : (
          stages.map((stage, idx) => (
            <div key={stage.id} style={{ display: 'flex', alignItems: 'center', gap: 0 }}>
              {/* 箭头连接（第一项前不显示） */}
              {idx > 0 && (
                <div style={{ display: 'flex', alignItems: 'center', padding: '0 4px' }}>
                  <ArrowRightOutlined style={{ color: 'var(--color-text-tertiary, #94a3b8)', fontSize: 16 }} />
                </div>
              )}

              {/* 环节卡片 */}
              <div
                draggable
                onDragStart={(e) => handleDragStart(e, idx)}
                onDragOver={(e) => handleDragOver(e, idx)}
                onDragEnd={handleDragEnd}
                onClick={() => handleOpenEdit(stage)}
                onMouseEnter={(e: React.MouseEvent<HTMLDivElement>) => {
                  setHoveredStageId(stage.id);
                  e.currentTarget.style.boxShadow = '0 4px 12px color-mix(in srgb, var(--color-text, #0f172a) 10%, transparent)';
                  e.currentTarget.style.borderColor = 'var(--color-primary, #0891b2)';
                  e.currentTarget.style.transform = 'translateY(-2px)';
                }}
                onMouseLeave={(e: React.MouseEvent<HTMLDivElement>) => {
                  setHoveredStageId(null);
                  e.currentTarget.style.boxShadow = 'none';
                  e.currentTarget.style.borderColor = 'var(--color-border, #e2e8f0)';
                  e.currentTarget.style.transform = 'translateY(0)';
                }}
                style={{
                  position: 'relative',
                  width: 200, minWidth: 200,
                  background: 'var(--color-bg-elevated, #ffffff)',
                  border: '1px solid var(--color-border, #e2e8f0)',
                  borderRadius: 10,
                  padding: '14px 16px',
                  cursor: 'pointer',
                  transition: 'box-shadow 200ms, border-color 200ms, transform 200ms',
                  userSelect: 'none',
                }}
              >
                {/* 拖拽手柄 */}
                <div
                  style={{
                    position: 'absolute', top: 4, right: 4,
                    color: 'var(--color-text-tertiary, #94a3b8)',
                    fontSize: 12, cursor: 'grab', opacity: 0.5,
                  }}
                  onMouseEnter={(e) => { e.currentTarget.style.opacity = '1'; }}
                  onMouseLeave={(e) => { e.currentTarget.style.opacity = '0.5'; }}
                >
                  <DragOutlined />
                </div>

                {/* 序号：大号淡出数字背景 */}
                <div style={{
                  fontSize: 32, fontWeight: 800,
                  color: 'color-mix(in srgb, var(--color-primary, #0891b2) 12%, transparent)',
                  lineHeight: 1, marginBottom: 4, fontFamily: 'monospace',
                }}>
                  {String(idx + 1).padStart(2, '0')}
                </div>

                {/* 环节名称 */}
                <div style={{
                  fontWeight: 600, fontSize: 14,
                  color: 'var(--color-text, #0f172a)',
                  marginBottom: 4,
                  overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap',
                }}>
                  {stage.name}
                </div>

                {/* 关联 todo */}
                <div style={{
                  fontSize: 12,
                  color: 'var(--color-text-secondary, #475569)',
                  marginBottom: 10,
                  overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap',
                }}>
                  {stage.todo_title || `#${stage.todo_id}`}
                </div>

                {/* 底部：执行者 + 顺序执行标签 */}
                <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
                  <span style={{
                    display: 'inline-flex', alignItems: 'center', justifyContent: 'center',
                    width: 20, height: 20, borderRadius: 10,
                    background: 'var(--color-primary-bg, #f0f9ff)',
                    color: 'var(--color-primary, #0891b2)',
                    fontSize: 10, fontWeight: 600, flexShrink: 0,
                  }}>
                    {stage.todo_executor ? stage.todo_executor.charAt(0).toUpperCase() : '?'}
                  </span>
                  <span style={{
                    fontSize: 11, color: 'var(--color-text-tertiary, #94a3b8)',
                    flex: 1, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap',
                  }}>
                    {stage.todo_executor || '未指派'}
                  </span>
                  {stage.run_mode === 'sequential' && (
                    <span style={{
                      fontSize: 10, padding: '1px 6px', borderRadius: 4,
                      background: 'var(--color-bg-hover, #f1f5f9)',
                      color: 'var(--color-text-tertiary, #94a3b8)',
                      whiteSpace: 'nowrap',
                    }}>
                      顺序执行
                    </span>
                  )}
                </div>

                {/* 右上角启用状态指示点 */}
                <div style={{ position: 'absolute', top: 10, right: 28 }}>
                  <span style={{
                    display: 'inline-block', width: 8, height: 8, borderRadius: 4,
                    background: stage.enabled ? 'var(--color-success, #22c55e)' : 'var(--color-text-tertiary, #94a3b8)',
                  }} />
                </div>

                {/* hover 时显示删除按钮 */}
                <div
                  style={{
                    position: 'absolute', bottom: 8, right: 8,
                    opacity: hoveredStageId === stage.id ? 1 : 0,
                    transition: 'opacity 150ms',
                  }}
                  onClick={(e) => e.stopPropagation()}
                >
                  <Popconfirm
                    title="删除环节"
                    description={`确定删除「${stage.name}」？`}
                    onConfirm={() => handleDelete(stage.id)}
                    okText="确定"
                    cancelText="取消"
                  >
                    <Button
                      size="small"
                      danger
                      icon={<DeleteOutlined />}
                      style={{ fontSize: 11, padding: '0 4px', minWidth: 0 }}
                    />
                  </Popconfirm>
                </div>
              </div>
            </div>
          ))
        )}

        {/* 添加按钮 */}
        {stages.length > 0 && (
          <div style={{ display: 'flex', alignItems: 'center', gap: 0 }}>
            <div style={{ display: 'flex', alignItems: 'center', padding: '0 4px' }}>
              <ArrowRightOutlined style={{ color: 'var(--color-text-tertiary, #94a3b8)', fontSize: 16 }} />
            </div>
            <div
              onClick={handleOpenAdd}
              role="button"
              tabIndex={0}
              onKeyDown={(e) => { if (e.key === 'Enter') handleOpenAdd(); }}
              style={{
                display: 'flex', flexDirection: 'column', alignItems: 'center', justifyContent: 'center',
                width: 200, minWidth: 200, minHeight: 160,
                border: '2px dashed var(--color-border, #e2e8f0)',
                borderRadius: 10, cursor: 'pointer',
                color: 'var(--color-text-tertiary, #94a3b8)',
                fontSize: 13, gap: 8,
                transition: 'border-color 200ms, color 200ms, background 200ms',
              }}
              onMouseEnter={(e) => {
                e.currentTarget.style.borderColor = 'var(--color-primary, #0891b2)';
                e.currentTarget.style.color = 'var(--color-primary, #0891b2)';
                e.currentTarget.style.background = 'var(--color-primary-bg, #f0f9ff)';
              }}
              onMouseLeave={(e) => {
                e.currentTarget.style.borderColor = 'var(--color-border, #e2e8f0)';
                e.currentTarget.style.color = 'var(--color-text-tertiary, #94a3b8)';
                e.currentTarget.style.background = 'transparent';
              }}
            >
              <PlusOutlined style={{ fontSize: 24 }} />
              <span>添加环节</span>
            </div>
          </div>
        )}
      </div>

      {/* 新增 / 编辑 Modal */}
      <Modal
        title={editingStage ? '编辑环节' : '新增环节'}
        open={modalOpen}
        onCancel={() => setModalOpen(false)}
        onOk={handleSave}
        okText={editingStage ? '保存' : '添加'}
        cancelText="取消"
        confirmLoading={saving}
        width={520}
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
        </Form>
      </Modal>
    </>
  );
}
