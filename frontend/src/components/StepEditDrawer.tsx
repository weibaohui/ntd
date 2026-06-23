// 环节编辑 Drawer：封装编辑表单的所有状态和交互逻辑。
// 将大组件拆分为：数据加载 hook + 编辑 Drawer + 各展示区段，
// 每个子组件体均控制在 30 行以内。

import { useState, useCallback, useRef, useMemo } from 'react';
import { Drawer, Input, Divider, App as AntApp } from 'antd';
import { Button } from 'antd';
import { ExecutorPicker } from '@/components/todo-drawer/ExecutorPicker';
import { PromptEditor } from '@/components/todo-drawer/PromptEditor';
import { SkillSelector } from '@/components/todo-drawer/SkillSelector';
import { TemplateModal } from '@/components/todo-drawer/TemplateModal';
import { TagCheckCardGroup } from './TagCheckCard';
import * as dbSteps from '@/utils/database/steps';
import * as db from '@/utils/database';
import type { StepSummary, SkillMeta, ExecutorSkills, TodoTemplate } from '@/types';
import { EXECUTORS_FOR_PICKER, getExecutorColor } from '@/types';

// 编辑器 ref 接口：描述 MdEditor 暴露的方法，用于替代 any 类型，
// 提高类型安全性，避免隐式 any 带来的潜在错误。
interface MdEditorRef {
  insertText: (text: string) => void;
}

interface StepEditDrawerProps {
  open: boolean;
  step: StepSummary;
  /** 可用标签列表（复用 Todo 的标签体系） */
  tags: Array<{ id: number; name: string; color: string }>;
  onClose: () => void;
  onSaved: () => void;
}

// 编辑表单 hook：集中管理标题、提示词、执行器、标签、验收标准等状态，
// 以及 skills / 模板的加载和交互逻辑，降低主组件的 useState 数量。
function useEditForm(step: StepSummary) {
  const { message } = AntApp.useApp();
  const [editTitle, setEditTitle] = useState('');
  const [editPrompt, setEditPrompt] = useState('');
  const [editExecutor, setEditExecutor] = useState('');
  // 标签状态：单选，仅允许选中一个标签（与 Todo 保持一致）
  const [selectedTag, setSelectedTag] = useState<number | null>(null);
  const [editAcceptanceCriteria, setEditAcceptanceCriteria] = useState('');
  const [saving, setSaving] = useState(false);
  // 使用具体的编辑器 ref 类型而非 any，提高类型安全性，
  // 与 MdEditor 组件暴露的接口保持一致。
  const editorRef = useRef<MdEditorRef | null>(null);

  // Skills 相关状态
  const [skillsLoading, setSkillsLoading] = useState(false);
  const [skillsExpanded, setSkillsExpanded] = useState(false);
  const [skillSearchText, setSkillSearchText] = useState('');
  const [allExecutorSkills, setAllExecutorSkills] = useState<ExecutorSkills[]>([]);

  // 模板相关状态
  const [templateModalOpen, setTemplateModalOpen] = useState(false);
  const [templates, setTemplates] = useState<TodoTemplate[]>([]);
  const [templatesLoading, setTemplatesLoading] = useState(false);

  // 当前执行器对应的 skills
  const currentSkills = useMemo(() => {
    const found = allExecutorSkills.find((e: any) => e.executor === editExecutor);
    return found?.skills || [];
  }, [editExecutor, allExecutorSkills]);

  const executorColor = getExecutorColor(editExecutor);

  // 从 step 初始化编辑表单，同时预加载 skills 列表
  const initFromStep = useCallback(() => {
    setEditTitle(step.title);
    setEditPrompt(step.prompt);
    setEditExecutor(step.executor || '');
    // 初始化标签：从 step.tag_ids 取第一个（单选）
    setSelectedTag(step.tag_ids?.[0] ?? null);
    setEditAcceptanceCriteria(step.acceptance_criteria || '');
    setSkillsExpanded(false);
    setSkillSearchText('');
    setSkillsLoading(true);
    db.getSkillsList()
      .then((data) => setAllExecutorSkills(data))
      .catch(() => {})
      .finally(() => setSkillsLoading(false));
  }, [step]);

  // 光标插入文本：优先使用编辑器的 insertText 方法保持光标位置，
  // 否则退化为追加到末尾。
  const insertTextAtCursor = useCallback((text: string) => {
    const editor = editorRef.current;
    if (editor?.insertText) {
      editor.insertText(text);
    } else {
      setEditPrompt(prev => prev + text);
    }
  }, []);

  const handleSkillClick = useCallback((skill: SkillMeta) => {
    insertTextAtCursor(`/${skill.name}`);
  }, [insertTextAtCursor]);

  // 加载模板列表并打开模板 Modal
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

  // 选择模板后将 prompt 和 title 插入表单
  const handleSelectTemplate = useCallback((template: TodoTemplate) => {
    if (template.prompt) insertTextAtCursor(template.prompt);
    if (template.title) setEditTitle(prev => prev || template.title);
    setTemplateModalOpen(false);
    message.success(`已应用模板「${template.title}」`);
  }, [insertTextAtCursor, message]);

  return {
    message, editTitle, setEditTitle, editPrompt, setEditPrompt,
    editExecutor, setEditExecutor, selectedTag, setSelectedTag,
    editAcceptanceCriteria, setEditAcceptanceCriteria, saving, setSaving,
    editorRef, skillsLoading, skillsExpanded, setSkillsExpanded,
    skillSearchText, setSkillSearchText, currentSkills, executorColor,
    templateModalOpen, setTemplateModalOpen, templates, templatesLoading,
    initFromStep, insertTextAtCursor, handleSkillClick,
    handleOpenTemplate, handleSelectTemplate,
  };
}

// 编辑 Drawer 的标题输入区：独立组件避免主组件过长
function EditTitleSection({ value, onChange }: { value: string; onChange: (v: string) => void }) {
  return (
    <div style={{ padding: '16px 20px', borderBottom: '1px solid var(--color-border-light)' }}>
      <Input
        value={value}
        onChange={e => onChange(e.target.value)}
        placeholder="环节标题"
        style={{ fontSize: 16, fontWeight: 600, padding: '8px 12px' }}
      />
    </div>
  );
}

// 编辑 Drawer 的滚动内容区：执行器、标签、Prompt、Skills、验收标准
// 将长表单拆为独立区段，便于阅读和维护
function EditContentSection({ form, tags }: { form: ReturnType<typeof useEditForm>; tags: StepEditDrawerProps['tags'] }) {
  return (
    <div style={{ flex: 1, overflow: 'auto', padding: '16px 20px' }}>
      <ExecutorPicker
        executor={form.editExecutor}
        executorOptions={EXECUTORS_FOR_PICKER}
        onChange={form.setEditExecutor}
      />
      <TagSection form={form} tags={tags} />
      <Divider style={{ margin: '8px 0 16px' }} />
      <PromptSection form={form} />
      <SkillsSection form={form} />
      <Divider style={{ margin: '8px 0 16px' }} />
      <AcceptanceCriteriaSection value={form.editAcceptanceCriteria} onChange={form.setEditAcceptanceCriteria} />
    </div>
  );
}

// 标签选择区段（复用 Todo 的标签体系，单选）
function TagSection({ form, tags }: { form: ReturnType<typeof useEditForm>; tags: StepEditDrawerProps['tags'] }) {
  if (tags.length === 0) return null;
  return (
    <div style={{ marginBottom: 16 }}>
      <div style={{ marginBottom: 10, fontWeight: 600, fontSize: 14 }}>标签</div>
      <TagCheckCardGroup
        tags={tags}
        value={form.selectedTag}
        onChange={(val) => form.setSelectedTag(val as number | null)}
      />
    </div>
  );
}

// Prompt 编辑器区段
function PromptSection({ form }: { form: ReturnType<typeof useEditForm> }) {
  return (
    <>
      <PromptEditor
        value={form.editPrompt}
        onChange={form.setEditPrompt}
        editorRef={form.editorRef}
        onOpenTemplate={form.handleOpenTemplate}
        onInsertText={form.insertTextAtCursor}
      />
      <LoopVariableHints onInsert={form.insertTextAtCursor} />
    </>
  );
}

// Loop 环节模板变量提示
const LOOP_VARS = [
  { label: '{{message}}', desc: '上一环节的完整输出（最常用）' },
  { label: '{{last_output}}', desc: '上一环节的完整输出（同 message）' },
  { label: '{{last_conclusion}}', desc: '上一环节的结论摘要' },
  { label: '{{last_step_name}}', desc: '上一环节的名称' },
  { label: '{{blackboard}}', desc: '全部历史执行记录（结论黑板）' },
  { label: '{{loop_execution_id}}', desc: '本次 Loop 执行 ID' },
  { label: '{{loop_name}}', desc: 'Loop 名称' },
];

function LoopVariableHints({ onInsert }: { onInsert: (text: string) => void }) {
  const [expanded, setExpanded] = useState(false);
  return (
    <div style={{ marginTop: 8, fontSize: 12, color: '#64748b' }}>
      <div
        onClick={() => setExpanded(!expanded)}
        style={{ cursor: 'pointer', userSelect: 'none', display: 'flex', alignItems: 'center', gap: 4 }}
      >
        <span style={{ fontWeight: 600 }}>
          {expanded ? '▼' : '▶'} 模板变量
        </span>
        <span style={{ fontSize: 11, color: '#94a3b8' }}>（点击插入到提示词中）</span>
      </div>
      {expanded && (
        <div style={{ marginTop: 6, display: 'flex', flexWrap: 'wrap', gap: 4 }}>
          {LOOP_VARS.map(v => (
            <span
              key={v.label}
              onClick={() => onInsert(v.label)}
              title={v.desc}
              style={{
                display: 'inline-flex', alignItems: 'center', gap: 4,
                padding: '2px 8px', borderRadius: 4,
                background: '#f1f5f9', border: '1px solid #e2e8f0',
                cursor: 'pointer', fontSize: 11, fontFamily: 'monospace',
                transition: 'background 150ms',
              }}
              onMouseEnter={(e) => { e.currentTarget.style.background = '#e2e8f0'; }}
              onMouseLeave={(e) => { e.currentTarget.style.background = '#f1f5f9'; }}
            >
              <code style={{ fontSize: 11, color: '#0891b2' }}>{v.label}</code>
              <span style={{ fontSize: 10, color: '#94a3b8' }}>{v.desc}</span>
            </span>
          ))}
        </div>
      )}
    </div>
  );
}

// Skills 选择区段
function SkillsSection({ form }: { form: ReturnType<typeof useEditForm> }) {
  return (
    <SkillSelector
      skills={form.currentSkills}
      loading={form.skillsLoading}
      executorColor={form.executorColor}
      searchText={form.skillSearchText}
      onSearchChange={form.setSkillSearchText}
      expanded={form.skillsExpanded}
      onToggle={() => form.setSkillsExpanded(!form.skillsExpanded)}
      onSkillClick={form.handleSkillClick}
    />
  );
}

// 验收标准输入区段
function AcceptanceCriteriaSection({ value, onChange }: { value: string; onChange: (v: string) => void }) {
  return (
    <div style={{ marginBottom: 16 }}>
      <div style={{ marginBottom: 8, fontWeight: 600, fontSize: 14 }}>验收标准</div>
      <Input.TextArea
        value={value}
        onChange={e => onChange(e.target.value)}
        placeholder="描述完成该环节需要满足的条件..."
        rows={3}
        style={{ resize: 'vertical' }}
      />
    </div>
  );
}

// 编辑 Drawer 主组件：打开时初始化表单，保存时调用 API
export function StepEditDrawer({ open, step, tags, onClose, onSaved }: StepEditDrawerProps) {
  const { message } = AntApp.useApp();
  const form = useEditForm(step);

  // 打开 Drawer 时从 step 初始化表单数据
  const handleAfterOpen = useCallback(() => {
    form.initFromStep();
  }, [form]);

  // 保存：校验 → 调用 API（基本信息 + 标签分开保存）→ 通知父组件
  const handleSave = useCallback(async () => {
    if (!form.editTitle.trim()) { message.error('标题不能为空'); return; }
    form.setSaving(true);
    try {
      // 一次性保存基本信息+标签，避免分两次 API 调用导致部分提交风险；
      // 后端 update_step handler 已支持 tag_ids 字段，收到后一并持久化标签关联
      await dbSteps.updateStep(step.id, {
        title: form.editTitle.trim(),
        prompt: form.editPrompt,
        executor: form.editExecutor || null,
        acceptance_criteria: form.editAcceptanceCriteria || null,
        tag_ids: form.selectedTag != null ? [form.selectedTag] : [],
      });
      message.success('环节已更新');
      onSaved();
      onClose();
    } catch {
      message.error('保存失败，请重试');
    } finally {
      form.setSaving(false);
    }
  }, [form, step.id, message, onSaved, onClose]);

  return (
    <>
      <Drawer
        title="编辑环节"
        open={open}
        onClose={onClose}
        afterOpenChange={(visible) => { if (visible) handleAfterOpen(); }}
        width={600}
        placement="right"
        destroyOnClose
        styles={{ body: { padding: 0 } }}
        extra={<Button type="primary" loading={form.saving} onClick={handleSave}>保存</Button>}
      >
        <EditTitleSection value={form.editTitle} onChange={form.setEditTitle} />
        <EditContentSection form={form} tags={tags} />
      </Drawer>
      <TemplateModal
        open={form.templateModalOpen}
        templates={form.templates}
        loading={form.templatesLoading}
        onClose={() => form.setTemplateModalOpen(false)}
        onSelect={form.handleSelectTemplate}
      />
    </>
  );
}
