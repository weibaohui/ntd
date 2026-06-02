import { useState, useEffect, useMemo, useRef, useCallback } from 'react';
import { Drawer, Input, Button, App, AutoComplete, Divider, Switch } from 'antd';
import { FolderOutlined } from '@ant-design/icons';
import * as db from '../utils/database';
import type { ProjectDirectory } from '../utils/database';
import type { TodoHookItem } from '../utils/database/hooks';
import type { Todo, ExecutorConfig, ExecutorOption, SkillMeta, ExecutorSkills, TodoTemplate } from '../types';
import { EXECUTORS, executorConfigToOption, getExecutorColor } from '../types';
import { TagCheckCardGroup } from './TagCheckCard';
import { ExecutorPicker } from './todo-drawer/ExecutorPicker';
import { PromptEditor } from './todo-drawer/PromptEditor';
import { SkillSelector } from './todo-drawer/SkillSelector';
import { SchedulerSection } from './todo-drawer/SchedulerSection';
import { TemplateModal } from './todo-drawer/TemplateModal';
import { TodoHooksEditor } from './todo-detail/TodoHooksEditor';
import { useApp } from '../hooks/useApp';

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

  const [title, setTitle] = useState('');
  const [prompt, setPrompt] = useState('');
  const [selectedTags, setSelectedTags] = useState<number[]>([]);
  const [executor, setExecutor] = useState<string>('claudecode');
  const [workspace, setWorkspace] = useState<string>('');
  const [worktreeEnabled, setWorktreeEnabled] = useState(false);
  const [executorOptions, setExecutorOptions] = useState<ExecutorOption[]>(EXECUTORS);
  const [projectDirectories, setProjectDirectories] = useState<ProjectDirectory[]>([]);
  const [allExecutorSkills, setAllExecutorSkills] = useState<ExecutorSkills[]>([]);
  const [skillsLoading, setSkillsLoading] = useState(false);
  const [skillsExpanded, setSkillsExpanded] = useState(false);
  const [skillSearchText, setSkillSearchText] = useState('');
  const [schedulerEnabled, setSchedulerEnabled] = useState(false);
  const [schedulerConfig, setSchedulerConfig] = useState<string>('');
  const [hooks, setHooks] = useState<TodoHookItem[]>([]);
  const [loading, setLoading] = useState(false);
  const editorRef = useRef<any>(null);
  const { state: appState } = useApp();

  const insertTextAtCursor = useCallback((text: string) => {
    const editor = editorRef.current;
    if (!editor || !editor.textarea) {
      setPrompt(prev => {
        if (!prev) return text;
        return prev + (prev.endsWith('\n') ? '' : '\n') + text;
      });
      return;
    }
    const textarea = editor.textarea as HTMLTextAreaElement;
    const start = textarea.selectionStart;
    const end = textarea.selectionEnd;
    setPrompt(prev => {
      return prev.substring(0, start) + text + prev.substring(end);
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
      Promise.all([
        db.getExecutors(),
        db.getProjectDirectories(),
      ]).then(([executorConfigs, directories]) => {
        const enabled = (executorConfigs as ExecutorConfig[]).filter((ec) => ec.enabled);
        if (enabled.length > 0) {
          setExecutorOptions(enabled.map(executorConfigToOption));
        }
        setProjectDirectories(directories);
      }).catch(() => {});

      setSkillsLoading(true);
      db.getSkillsList()
        .then((data) => setAllExecutorSkills(data))
        .catch(() => {})
        .finally(() => setSkillsLoading(false));
    }
  }, [open]);

  useEffect(() => {
    if (open) {
      if (todo) {
        setTitle(todo.title || '');
        setPrompt(todo.prompt || '');
        setSelectedTags((todo as any).tag_ids || []);
        setExecutor(todo.executor || 'claudecode');
        setWorkspace(todo.workspace || '');
        setWorktreeEnabled(todo.worktree_enabled || false);
        setSchedulerEnabled(todo.scheduler_enabled || false);
        setSchedulerConfig(todo.scheduler_config || '');
        setHooks(todo.hooks ?? []);
      } else {
        setTitle('');
        setPrompt('');
        setSelectedTags([]);
        setExecutor('claudecode');
        setWorkspace('');
        setWorktreeEnabled(false);
        setSchedulerEnabled(false);
        setSchedulerConfig('');
        setHooks([]);
      }
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
      setTitle(template.title);
      setPrompt(template.prompt || '');
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
  }, [prompt, insertTextAtCursor, message]);

  const handleSave = async () => {
    if (!title.trim()) {
      message.error('请输入任务标题');
      return;
    }

    setLoading(true);
    try {
      const trimmedWorkspace = workspace.trim() || null;

      if (isEditMode && todo) {
        if (trimmedWorkspace) {
          const exists = projectDirectories.some(d => d.path === trimmedWorkspace);
          if (!exists) {
            try { await db.createProjectDirectory(trimmedWorkspace); } catch { }
          }
        }

        await db.updateTodo(
          todo.id, title.trim(), prompt.trim(), todo.status,
          executor, schedulerEnabled, schedulerConfig || null,
          trimmedWorkspace, worktreeEnabled,
          hooks,
        );
        await db.updateScheduler(todo.id, schedulerEnabled, schedulerConfig || null);
        await db.updateTodoTags(todo.id, selectedTags);
        message.success('任务已更新');
      } else {
        const newTodo = await db.createTodo(title.trim(), prompt.trim(), selectedTags, hooks);

        if (trimmedWorkspace || schedulerEnabled || executor !== 'claudecode' || worktreeEnabled) {
          if (trimmedWorkspace) {
            const exists = projectDirectories.some(d => d.path === trimmedWorkspace);
            if (!exists) {
              try { await db.createProjectDirectory(trimmedWorkspace); } catch { }
            }
          }
          await db.updateTodo(
            newTodo.id, newTodo.title, newTodo.prompt, newTodo.status,
            executor, schedulerEnabled, schedulerConfig || null,
            trimmedWorkspace, worktreeEnabled,
            hooks,
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
            onChange={e => setTitle(e.target.value)}
            placeholder="任务标题"
            style={{ fontSize: 16, fontWeight: 600, padding: '8px 12px' }}
          />
        </div>

        <div style={{ flex: 1, overflow: 'auto', padding: '16px 20px' }}>
          <ExecutorPicker executor={executor} executorOptions={executorOptions} onChange={setExecutor} />

          <Divider style={{ margin: '8px 0 16px' }} />

          <PromptEditor
            value={prompt}
            onChange={setPrompt}
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
                  onChange={(val) => setSelectedTags(val ? [val as number] : [])}
                />
              </div>
            </>
          )}

          <Divider style={{ margin: '8px 0 16px' }} />

          <div style={{ marginBottom: 16 }}>
            <div style={{ marginBottom: 10, fontWeight: 600, fontSize: 14 }}>
              <FolderOutlined style={{ color: 'var(--color-primary)', marginRight: 6 }} />
              工作目录
            </div>
            <AutoComplete
              value={workspace}
              onChange={(value) => setWorkspace(value)}
              options={projectDirectories.map(d => ({
                value: d.path,
                label: d.name ? `${d.name} (${d.path})` : d.path,
              }))}
              placeholder="从项目目录选择或手动输入路径"
              style={{ width: '100%' }}
              filterOption={(input, option) =>
                (option?.label as string)?.toLowerCase().includes(input.toLowerCase())
              }
            />
          </div>

          {(executor === 'claudecode' || executor === 'claude_code' || executor === 'hermes') && (
            <div style={{ marginBottom: 16 }}>
              <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
                <div style={{ fontWeight: 600, fontSize: 14 }}>Git Worktree</div>
                <Switch checked={worktreeEnabled} onChange={(checked) => setWorktreeEnabled(checked)} />
              </div>
            </div>
          )}

          <Divider style={{ margin: '8px 0 16px' }} />

          <SchedulerSection
            enabled={schedulerEnabled}
            config={schedulerConfig}
            onEnabledChange={setSchedulerEnabled}
            onConfigChange={setSchedulerConfig}
            existingConfig={todo?.scheduler_config}
          />

          <Divider style={{ margin: '8px 0 16px' }} />

          <TodoHooksEditor
            todos={appState.todos}
            ownerId={todo?.id ?? null}
            hooks={hooks}
            onChange={setHooks}
            disabled={loading}
          />
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
