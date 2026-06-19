// 环节详情面板 + 编辑功能。
// 编辑交互与 TodoDrawer 一致：Drawer 右侧滑出，Prompt/Skill/Template 完全复用。

import { useEffect, useState, useCallback, useRef, useMemo } from 'react';
import {
  Skeleton, Empty, Tag, Descriptions, Button, Drawer, Input, Divider, ColorPicker, Popconfirm, App as AntApp,
} from 'antd';
import { ApartmentOutlined, ThunderboltOutlined, EditOutlined, DeleteOutlined } from '@ant-design/icons';
import { ExecutorPicker } from '@/components/todo-drawer/ExecutorPicker';
import { PromptEditor } from '@/components/todo-drawer/PromptEditor';
import { SkillSelector } from '@/components/todo-drawer/SkillSelector';
import { TemplateModal } from '@/components/todo-drawer/TemplateModal';
import * as dbSteps from '@/utils/database/steps';
import * as db from '@/utils/database';
import type { StepSummary, SkillMeta, ExecutorSkills, TodoTemplate } from '@/types';
import { EXECUTORS_FOR_PICKER, getExecutorColor } from '@/types';
import { formatRelativeTime } from '@/utils/datetime';

interface StepDetailPanelProps {
  stepId: number;
  // 保存成功后通知父组件刷新列表，保持左右栏数据同步
  onStepUpdated?: () => void;
}

export function StepDetailPanel({ stepId, onStepUpdated }: StepDetailPanelProps) {
  const { message } = AntApp.useApp();
  const [step, setStep] = useState<StepSummary | null>(null);
  const [loading, setLoading] = useState(true);
  const [editing, setEditing] = useState(false);
  const [saving, setSaving] = useState(false);

  // 编辑表单状态
  const [editTitle, setEditTitle] = useState('');
  const [editPrompt, setEditPrompt] = useState('');
  const [editExecutor, setEditExecutor] = useState('');
  const [editColor, setEditColor] = useState('#722ed1');
  const [editAcceptanceCriteria, setEditAcceptanceCriteria] = useState('');

  // Prompt 编辑器 ref（用于光标插入）
  const editorRef = useRef<any>(null);

  // Skills
  const [skillsLoading, setSkillsLoading] = useState(false);
  const [skillsExpanded, setSkillsExpanded] = useState(false);
  const [skillSearchText, setSkillSearchText] = useState('');
  const [allExecutorSkills, setAllExecutorSkills] = useState<ExecutorSkills[]>([]);

  // 模板
  const [templateModalOpen, setTemplateModalOpen] = useState(false);
  const [templates, setTemplates] = useState<TodoTemplate[]>([]);
  const [templatesLoading, setTemplatesLoading] = useState(false);

  // 当前执行器对应的 skills
  const currentSkills = useMemo(() => {
    const found = allExecutorSkills.find((e: any) => e.executor === editExecutor);
    return found?.skills || [];
  }, [editExecutor, allExecutorSkills]);

  const executorColor = getExecutorColor(editExecutor);

  const loadStep = useCallback(() => {
    setLoading(true);
    dbSteps.getStep(stepId)
      .then(setStep)
      .catch(() => setStep(null))
      .finally(() => setLoading(false));
  }, [stepId]);

  useEffect(() => { loadStep(); }, [loadStep]);

  const handleOpenEdit = useCallback(() => {
    if (!step) return;
    setEditTitle(step.title);
    setEditPrompt(step.prompt);
    setEditExecutor(step.executor || '');
    setEditColor(step.color || '#722ed1');
    setEditAcceptanceCriteria(step.acceptance_criteria || '');
    setSkillsExpanded(false);
    setSkillSearchText('');

    // 加载 skills
    setSkillsLoading(true);
    db.getSkillsList()
      .then((data) => setAllExecutorSkills(data))
      .catch(() => {})
      .finally(() => setSkillsLoading(false));

    setEditing(true);
  }, [step]);

  const handleDelete = useCallback(async () => {
    if (!step) return;
    try {
      await dbSteps.deleteStep(step.id);
      message.success('环节已删除');
      onStepUpdated?.();
    } catch {
      message.error('删除失败，环节可能正在被 loop 引用');
    }
  }, [step, message, onStepUpdated]);

  // 光标插入文本
  const insertTextAtCursor = useCallback((text: string) => {
    const editor = editorRef.current;
    if (editor?.insertText) {
      editor.insertText(text);
    } else {
      setEditPrompt(prev => prev + text);
    }
  }, []);

  // 技能点击 → 插入 /skill_name
  const handleSkillClick = useCallback((skill: SkillMeta) => {
    insertTextAtCursor(`/${skill.name}`);
  }, [insertTextAtCursor]);

  // 模板
  const loadTemplates = useCallback(() => {
    setTemplatesLoading(true);
    db.getTodoTemplates()
      .then(setTemplates)
      .catch(() => message.error('加载模板失败'))
      .finally(() => setTemplatesLoading(false));
  }, [message]);

  const handleOpenTemplate = useCallback(() => {
    loadTemplates();
    setTemplateModalOpen(true);
  }, [loadTemplates]);

  const handleSelectTemplate = useCallback((template: TodoTemplate) => {
    if (template.prompt) {
      insertTextAtCursor(template.prompt);
    }
    if (template.title) {
      setEditTitle(prev => prev || template.title);
    }
    setTemplateModalOpen(false);
    message.success(`已应用模板「${template.title}」`);
  }, [insertTextAtCursor, message]);

  const handleSave = useCallback(async () => {
    if (!editTitle.trim()) { message.error('标题不能为空'); return; }
    setSaving(true);
    try {
      const updated = await dbSteps.updateStep(stepId, {
        title: editTitle.trim(),
        prompt: editPrompt,
        executor: editExecutor || null,
        acceptance_criteria: editAcceptanceCriteria || null,
        color: editColor,
      });
      setStep(updated);
      message.success('环节已更新');
      setEditing(false);
      // 通知父组件刷新列表，保持左栏数据同步
      onStepUpdated?.();
    } catch {
      // 保存失败时给用户明确反馈，而不是静默吞掉错误
      message.error('保存失败，请重试');
    } finally {
      setSaving(false);
    }
  }, [editTitle, editPrompt, editExecutor, editColor, editAcceptanceCriteria, stepId, message, onStepUpdated]);

  if (loading) {
    return <Skeleton active style={{ padding: 24 }} />;
  }
  if (!step) {
    return <Empty description="无法加载该环节" style={{ marginTop: 64 }} />;
  }

  return (
    <>
      <div style={{ padding: '20px 24px' }}>
        {/* Header */}
        <div style={{ display: 'flex', alignItems: 'center', gap: 12, marginBottom: 20 }}>
          <h2 style={{ margin: 0, fontSize: 18, flex: 1, minWidth: 0, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap', color: 'var(--color-text, #0f172a)' }}>
            {step.title}
          </h2>
          <span style={{ color: 'var(--color-text-tertiary, #94a3b8)', fontSize: 12, fontFamily: 'monospace' }}>#{step.id}</span>
          <Button size="small" icon={<EditOutlined />} onClick={handleOpenEdit}>编辑</Button>
          <Popconfirm
            title="删除环节"
            description="删除后无法恢复"
            okType="danger"
            onConfirm={handleDelete}
          >
            <Button size="small" danger icon={<DeleteOutlined />} />
          </Popconfirm>
        </div>

        {/* 基本信息 */}
        <section style={{
          background: 'var(--color-bg-elevated, #ffffff)',
          border: '1px solid var(--color-border, #e2e8f0)',
          borderRadius: 8,
          padding: 16,
          marginBottom: 12,
        }}>
          <div style={{ fontSize: 14, fontWeight: 600, color: 'var(--color-text, #0f172a)', marginBottom: 12 }}>基本信息</div>
          <Descriptions column={2} size="small" bordered={false}>
            <Descriptions.Item label="执行器">
              {step.executor ? (
                <span><ThunderboltOutlined style={{ color: '#fa8c16', marginRight: 4 }} />{step.executor}</span>
              ) : (
                <span style={{ color: 'var(--color-text-tertiary, #94a3b8)' }}>未指派</span>
              )}
            </Descriptions.Item>
            <Descriptions.Item label="复用次数">
              <Tag icon={<ApartmentOutlined />} color={step.used_by_loop_step_count > 0 ? 'purple' : 'default'}>
                {step.used_by_loop_step_count}
              </Tag>
            </Descriptions.Item>
            <Descriptions.Item label="来源事项">
              {step.source_todo_id ? (
                <span>#<code>{step.source_todo_id}</code></span>
              ) : (
                <span style={{ color: 'var(--color-text-tertiary, #94a3b8)' }}>—</span>
              )}
            </Descriptions.Item>
            <Descriptions.Item label="更新于">
              {step.updated_at ? formatRelativeTime(step.updated_at) : '—'}
            </Descriptions.Item>
          </Descriptions>
        </section>

        {/* Prompt */}
        <section style={{
          background: 'var(--color-bg-elevated, #ffffff)',
          border: '1px solid var(--color-border, #e2e8f0)',
          borderRadius: 8,
          padding: 16,
          marginBottom: 12,
        }}>
          <div style={{ fontSize: 14, fontWeight: 600, color: 'var(--color-text, #0f172a)', marginBottom: 8 }}>提示词 (Prompt)</div>
          <div style={{
            fontSize: 13, color: 'var(--color-text-secondary, #475569)',
            background: 'var(--color-bg-secondary, #f8fafc)',
            padding: 12, borderRadius: 6, whiteSpace: 'pre-wrap',
            lineHeight: 1.6,
          }}>
            {step.prompt || <span style={{ color: 'var(--color-text-tertiary, #94a3b8)' }}>无提示词</span>}
          </div>
        </section>

        {/* 验收标准 */}
        <section style={{
          background: 'var(--color-bg-elevated, #ffffff)',
          border: '1px solid var(--color-border, #e2e8f0)',
          borderRadius: 8,
          padding: 16,
        }}>
          <div style={{ fontSize: 14, fontWeight: 600, color: 'var(--color-text, #0f172a)', marginBottom: 8 }}>验收标准</div>
          <div style={{
            fontSize: 13, color: 'var(--color-text-secondary, #475569)',
            whiteSpace: 'pre-wrap', minHeight: 40,
          }}>
            {step.acceptance_criteria || <span style={{ color: 'var(--color-text-tertiary, #94a3b8)' }}>无验收标准</span>}
          </div>
        </section>
      </div>

      {/* 编辑 Drawer — 与 TodoDrawer 完全对齐 */}
      <Drawer
        title="编辑环节"
        open={editing}
        onClose={() => setEditing(false)}
        width={600}
        placement="right"
        destroyOnClose
        styles={{ body: { padding: 0 } }}
        extra={
          <Button type="primary" loading={saving} onClick={handleSave}>
            保存
          </Button>
        }
      >
        {/* 标题 */}
        <div style={{ padding: '16px 20px', borderBottom: '1px solid var(--color-border-light)' }}>
          <Input
            value={editTitle}
            onChange={e => setEditTitle(e.target.value)}
            placeholder="环节标题"
            style={{ fontSize: 16, fontWeight: 600, padding: '8px 12px' }}
          />
        </div>

        {/* 滚动内容 */}
        <div style={{ flex: 1, overflow: 'auto', padding: '16px 20px' }}>
          {/* 执行器 */}
          <ExecutorPicker
            executor={editExecutor}
            executorOptions={EXECUTORS_FOR_PICKER}
            onChange={setEditExecutor}
          />

          {/* 颜色 */}
          <div style={{ marginBottom: 16 }}>
            <div style={{ marginBottom: 8, fontWeight: 600, fontSize: 14 }}>颜色</div>
            <ColorPicker
              value={editColor}
              onChange={(c: any) => setEditColor(c?.toHexString?.() ?? c)}
              showText
              format="hex"
            />
          </div>

          <Divider style={{ margin: '8px 0 16px' }} />

          {/* Prompt */}
          <PromptEditor
            value={editPrompt}
            onChange={setEditPrompt}
            editorRef={editorRef}
            onOpenTemplate={handleOpenTemplate}
            onInsertText={insertTextAtCursor}
          />

          {/* Skills */}
          <SkillSelector
            skills={currentSkills}
            loading={skillsLoading}
            executorColor={executorColor}
            searchText={skillSearchText}
            onSearchChange={setSkillSearchText}
            expanded={skillsExpanded}
            onToggle={() => setSkillsExpanded(!skillsExpanded)}
            onSkillClick={handleSkillClick}
          />

          <Divider style={{ margin: '8px 0 16px' }} />

          {/* 验收标准 */}
          <div style={{ marginBottom: 16 }}>
            <div style={{ marginBottom: 8, fontWeight: 600, fontSize: 14 }}>验收标准</div>
            <Input.TextArea
              value={editAcceptanceCriteria}
              onChange={e => setEditAcceptanceCriteria(e.target.value)}
              placeholder="描述完成该环节需要满足的条件..."
              rows={3}
              style={{ resize: 'vertical' }}
            />
          </div>
        </div>
      </Drawer>

      {/* 模板 Modal */}
      <TemplateModal
        open={templateModalOpen}
        templates={templates}
        loading={templatesLoading}
        onClose={() => setTemplateModalOpen(false)}
        onSelect={handleSelectTemplate}
      />
    </>
  );
}
