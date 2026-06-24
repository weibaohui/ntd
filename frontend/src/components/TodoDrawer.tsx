import { useState, useEffect, useMemo, useRef, useCallback, useReducer } from 'react';
import { Drawer, Input, Button, App, Divider } from 'antd';
import { FolderOutlined } from '@ant-design/icons';
import * as db from '@/utils/database';
import { WorkspaceSelect } from './common/WorkspaceSelect';

import type { Todo, ExecutorConfig, ExecutorOption, SkillMeta, ExecutorSkills, TodoTemplate } from '@/types';
import { EXECUTORS, executorConfigToOption, getExecutorColor, DEFAULT_EXECUTOR } from '@/types';
import { TagCheckCardGroup } from './TagCheckCard';
import { ExecutorPicker } from './todo-drawer/ExecutorPicker';
import { PromptEditor } from './todo-drawer/PromptEditor';
import { SkillSelector } from './todo-drawer/SkillSelector';
import { SchedulerSection } from './todo-drawer/SchedulerSection';
import { TemplateModal } from './todo-drawer/TemplateModal';
import {
  todoFormReducer,
  initialFormState,
  type TodoFormState,
  type TodoFormAction,
} from './todo-drawer/reducer';

interface TodoDrawerProps {
  open: boolean;
  todo: Todo | null;
  tags: Array<{ id: number; name: string; color: string }>;
  onClose: () => void;
  onSaved: (todo?: Todo) => void;
}

export function TodoDrawer({ open, todo, tags, onClose, onSaved }: TodoDrawerProps) {
  const { message } = App.useApp();
  const isEditMode = todo !== null;

  // 使用 useReducer 替代多个 useState，集中管理表单状态
  const [formState, dispatch] = useReducer(todoFormReducer, initialFormState);

  // UI 相关的状态（不属于表单数据）
  const [executorOptions, setExecutorOptions] = useState<ExecutorOption[]>(EXECUTORS);
  const [allExecutorSkills, setAllExecutorSkills] = useState<ExecutorSkills[]>([]);
  const [skillsLoading, setSkillsLoading] = useState(false);
  const [skillsExpanded, setSkillsExpanded] = useState(false);
  const [skillSearchText, setSkillSearchText] = useState('');
  const [loading, setLoading] = useState(false);
  const editorRef = useRef<any>(null);
  // 旧版 hook 编辑器消费 useApp 的 todos，todo hook 已整块移除后该 hook 不再需要。

  // 从 formState 中解构出常用的字段
  const {
    title, prompt, selectedTags, executor, workspace,
    schedulerEnabled, schedulerConfig, acceptanceCriteria,
  } = formState;

  // 设置单个字段的快捷函数
  // 泛型 K 确保 field/value 类型一致，但 TS 无法对泛型 dispatch 做 discriminated union 窄化，
  // 所以这里用 as TodoFormAction 做内部断言（外部调用方通过泛型约束保证类型安全）。
  const setField = useCallback(<K extends keyof TodoFormState>(
    field: K,
    value: TodoFormState[K],
  ) => {
    dispatch({ type: 'SET_FIELD', field, value } as TodoFormAction);
  }, []);

  const insertTextAtCursor = useCallback((text: string) => {
    const editor = editorRef.current;
    if (!editor || !editor.textarea) {
      // 无编辑器时：通过 functional updater 追加文本，避免 closure 捕获旧状态
      dispatch({
        type: 'SET_FIELD_UPDATER',
        field: 'prompt',
        updater: (prev: string) => prev
          ? prev + (prev.endsWith('\n') ? '' : '\n') + text
          : text,
      });
      return;
    }
    const textarea = editor.textarea as HTMLTextAreaElement;
    const start = textarea.selectionStart;
    const end = textarea.selectionEnd;
    // 使用 functional updater 在光标处插入文本，确保同一渲染周期内多次调用
    // 不会因 closure 捕获旧状态而丢失上一次的改动
    dispatch({
      type: 'SET_FIELD_UPDATER',
      field: 'prompt',
      updater: (prev: string) => prev.substring(0, start) + text + prev.substring(end),
    });
    setTimeout(() => {
      textarea.selectionStart = textarea.selectionEnd = start + text.length;
      textarea.focus();
    }, 0);
  }, []);

  const [templateModalOpen, setTemplateModalOpen] = useState(false);
  const [templates, setTemplates] = useState<TodoTemplate[]>([]);
  const [templatesLoading, setTemplatesLoading] = useState(false);

  const currentSkills = useMemo(() => {
    const found = allExecutorSkills.find(e => e.executor === executor);
    return found?.skills || [];
  }, [executor, allExecutorSkills]);

  useEffect(() => {
    if (open) {
      db.getExecutors().then((executorConfigs) => {
        const enabled = (executorConfigs as ExecutorConfig[]).filter((ec) => ec.enabled);
        if (enabled.length > 0) {
          setExecutorOptions(enabled.map(executorConfigToOption));
        }
      }).catch(() => {});

      setSkillsLoading(true);
      db.getSkillsList()
        .then((data) => setAllExecutorSkills(data))
        .catch(() => {})
        .finally(() => setSkillsLoading(false));
    }
  }, [open]);

  // 记录上一次传入的 todo，用于判断是否需要 RESET_FORM。
  // deps 保持 [open, todo]（检测内容引用变化），但内部用 id 比较
  // 避免仅在引用更新但内容未变时静默重置用户编辑。
  const prevTodoRef = useRef(todo);

  useEffect(() => {
    if (open) {
      const prevTodo = prevTodoRef.current;
      prevTodoRef.current = todo;
      // 如果 todo 引用变了但 id 相同（父组件重拉 todos 导致的新引用），
      // 不触发 RESET_FORM，保留用户在抽屉中的编辑
      if (prevTodo !== todo && todo && prevTodo?.id === todo.id) {
        return;
      }
      dispatch({ type: 'RESET_FORM', todo });
    }
  }, [open, todo]);

  const handleSkillClick = useCallback((skill: SkillMeta) => {
    insertTextAtCursor(`/${skill.name}`);
  }, [insertTextAtCursor]);

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
    if (!prompt.trim()) {
      setField('title', template.title);
      setField('prompt', template.prompt || '');
      message.success('已应用模板');
    } else {
      if (template.prompt) {
        insertTextAtCursor(template.prompt);
        message.success('已插入模板内容');
      } else {
        message.warning('模板内容为空，未插入');
      }
    }
    setTemplateModalOpen(false);
  }, [formState.prompt, insertTextAtCursor, message]);

  const handleSave = async () => {
    if (!title.trim()) {
      message.error('请输入任务标题');
      return;
    }

    // 创建模式：工作空间为必填项
    if (!isEditMode && !workspace.trim()) {
      message.error('请选择工作空间');
      return;
    }

    setLoading(true);
    try {
      const trimmedWorkspace = workspace.trim() || null;

      if (isEditMode && todo) {
        // WorkspaceSelect 只允许从下拉列表选择有效路径，无需二次校验
        await db.updateTodo(
          todo.id, title.trim(), prompt.trim(), todo.status,
          executor, schedulerEnabled, schedulerConfig || null,
          trimmedWorkspace,
          acceptanceCriteria || null,
        );
        await db.updateScheduler(todo.id, schedulerEnabled, schedulerConfig || null);
        await db.updateTodoTags(todo.id, selectedTags);
        message.success('任务已更新');
      } else {
        const newTodo = await db.createTodo(title.trim(), prompt.trim(), selectedTags, acceptanceCriteria || undefined);

        // WorkspaceSelect 只允许从下拉列表选择，无需二次校验
        const workspaceToSave = trimmedWorkspace;

        if (workspaceToSave || schedulerEnabled || executor !== DEFAULT_EXECUTOR) {
          await db.updateTodo(
            newTodo.id, newTodo.title, newTodo.prompt, newTodo.status,
            executor, schedulerEnabled, schedulerConfig || null,
            workspaceToSave,
            acceptanceCriteria || null,
          );
          await db.updateScheduler(newTodo.id, schedulerEnabled, schedulerConfig || null);
        }

        message.success('任务创建成功');
      }

      onSaved();
      onClose();
    } catch (error) {
      message.error('保存失败: ' + (error instanceof Error ? error.message : String(error)));
    } finally {
      setLoading(false);
    }
  };

  const executorColor = getExecutorColor(executor);

  return (
    <Drawer
      title={isEditMode ? '编辑任务' : '创建任务'}
      open={open}
      onClose={onClose}
      width={600}
      placement="right"
      destroyOnClose
      styles={{ body: { padding: 0 } }}
      extra={
        <Button type="primary" loading={loading} onClick={handleSave}>
          {isEditMode ? '保存' : '创建'}
        </Button>
      }
    >
      <div style={{ display: 'flex', flexDirection: 'column', height: '100%', background: 'var(--color-bg-elevated)' }}>
        <div style={{ padding: '16px 20px', borderBottom: '1px solid var(--color-border-light)' }}>
          <Input
            value={title}
            onChange={e => setField('title', e.target.value)}
            placeholder="任务标题"
            style={{ fontSize: 16, fontWeight: 600, padding: '8px 12px' }}
          />
        </div>

        <div style={{ flex: 1, overflow: 'auto', padding: '16px 20px' }}>
          <ExecutorPicker executor={executor} executorOptions={executorOptions} onChange={(v) => setField('executor', v)} />

          <Divider style={{ margin: '8px 0 16px' }} />

          <PromptEditor
            value={prompt}
            onChange={(v) => setField('prompt', v)}
            editorRef={editorRef}
            onOpenTemplate={handleOpenTemplate}
            onInsertText={insertTextAtCursor}
          />

          <SkillSelector
            skills={currentSkills}
            loading={skillsLoading}
            executorColor={executorColor}
            searchText={skillSearchText}
            onSearchChange={setSkillSearchText}
            expanded={skillsExpanded}
            onToggle={() => setSkillsExpanded(prev => !prev)}
            onSkillClick={handleSkillClick}
          />

          {tags.length > 0 && (
            <>
              <Divider style={{ margin: '8px 0 16px' }} />
              <div style={{ marginBottom: 16 }}>
                <div style={{ marginBottom: 10, fontWeight: 600, fontSize: 14 }}>标签</div>
                <TagCheckCardGroup
                  tags={tags}
                  value={selectedTags[0] || null}
                  onChange={(val) => setField('selectedTags', val ? [val as number] : [])}
                />
              </div>
            </>
          )}

          <Divider style={{ margin: '8px 0 16px' }} />

          <div style={{ marginBottom: 16 }}>
            <div style={{ marginBottom: 10, fontWeight: 600, fontSize: 14 }}>
              <FolderOutlined style={{ color: 'var(--color-primary)', marginRight: 6 }} />
              工作空间 <span style={{ color: '#ff4d4f' }}>*</span>
            </div>
            <WorkspaceSelect
              value={workspace}
              onChange={(v) => setField('workspace', v ?? '')}
              required
            />
          </div>



          <Divider style={{ margin: '8px 0 16px' }} />

          <SchedulerSection
            enabled={schedulerEnabled}
            config={schedulerConfig}
            onEnabledChange={(v) => setField('schedulerEnabled', v)}
            onConfigChange={(v) => setField('schedulerConfig', v)}
            existingConfig={todo?.scheduler_config}
          />

          <Divider style={{ margin: '8px 0 16px' }} />

          {/* 验收标准 */}
          <div style={{ marginBottom: 16 }}>
            <div style={{ marginBottom: 8, fontWeight: 600, fontSize: 14 }}>验收标准</div>
            <Input.TextArea
              value={acceptanceCriteria}
              onChange={e => setField('acceptanceCriteria', e.target.value)}
              placeholder="描述完成该任务需要满足的条件..."
              rows={3}
              style={{ resize: 'vertical' }}
            />
          </div>

        </div>
      </div>

      <TemplateModal
        open={templateModalOpen}
        templates={templates}
        loading={templatesLoading}
        onClose={() => setTemplateModalOpen(false)}
        onSelect={handleSelectTemplate}
      />

    </Drawer>
  );
}
