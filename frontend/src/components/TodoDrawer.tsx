import { useState, useEffect, useMemo, useRef, useCallback } from 'react';
import { Drawer, Input, Button, App, AutoComplete, Divider, Switch, Modal, Form, Empty, Space } from 'antd';
import { FolderOutlined, PlusOutlined } from '@ant-design/icons';
import * as db from '@/utils/database';
import type { ProjectDirectory } from '@/utils/database';
import type { TodoHookItem } from '@/utils/database/hooks';
import type { Todo, ExecutorConfig, ExecutorOption, SkillMeta, ExecutorSkills, TodoTemplate } from '@/types';
import { EXECUTORS, executorConfigToOption, getExecutorColor } from '@/types';
import { TagCheckCardGroup } from './TagCheckCard';
import { ExecutorPicker } from './todo-drawer/ExecutorPicker';
import { PromptEditor } from './todo-drawer/PromptEditor';
import { SkillSelector } from './todo-drawer/SkillSelector';
import { SchedulerSection } from './todo-drawer/SchedulerSection';
import { TemplateModal } from './todo-drawer/TemplateModal';
import { TodoHooksEditor } from './todo-detail/TodoHooksEditor';
import { useApp } from '@/hooks/useApp';

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
  // workspace 存的是路径；与 ProjectDirectory.path 对应，确保值可以直接命中已配置的目录
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
  const [acceptanceCriteria, setAcceptanceCriteria] = useState('');
  const [loading, setLoading] = useState(false);
  // 快速新增项目目录的弹窗：与抽屉同级，关闭抽屉时一起清理
  const [quickAddOpen, setQuickAddOpen] = useState(false);
  const [quickAddForm] = Form.useForm<{ name: string; path: string }>();
  const [quickAddSubmitting, setQuickAddSubmitting] = useState(false);
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
        setAcceptanceCriteria(todo.acceptance_criteria ?? '');
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
        setAcceptanceCriteria('');
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

  // 快速新增项目目录：
  // 用户在工作目录区域点"+"即可补一个项目，无需跳到设置页。
  // 流程：校验表单 → 调用后端创建 → 更新本地目录列表 + 自动选中 → 通知其他组件刷新。
  // 注意：name 必填由 antd Form rules 保证，这里额外 trim 后判空做兜底。
  const handleQuickAddProjectDirectory = async () => {
    let values: { name: string; path: string };
    try {
      values = await quickAddForm.validateFields(); // 触发 antd 必填校验
    } catch {
      // antd 校验失败时会自行提示，这里直接返回
      return;
    }
    const name = values.name.trim(); // 去前后空格，与后端 trim 策略一致
    const path = values.path.trim();
    if (!name || !path) { // 兜底：防止 trim 后为空
      message.error('项目名称与目录路径均为必填');
      return;
    }
    setQuickAddSubmitting(true); // 防止重复提交
    try {
      const dir = await db.createProjectDirectory(path, name); // 调用后端创建目录
      setProjectDirectories(prev =>
        [...prev.filter(d => d.id !== dir.id), dir].sort((a, b) => a.path.localeCompare(b.path)) // 去重+按路径排序
      );
      // 保存后立即把新目录选中并写入工作目录，减少二次操作
      setWorkspace(dir.path); // 自动选中新目录，减少用户二次操作
      setQuickAddOpen(false); // 关闭弹窗
      message.success(`已添加项目"${dir.name}"`);
      // 通知其他组件（如 TodoList 分组视图）项目目录已更新
      window.dispatchEvent(new CustomEvent('projectDirectoryAdded', { detail: dir })); // 通知 TodoList/KanbanBoard 等刷新
    } catch (err: any) {
      message.error('添加项目目录失败: ' + (err?.message || String(err)));
    } finally {
      setQuickAddSubmitting(false); // 无论成功失败都恢复按钮状态
    }
  };

  const handleSave = async () => {
    if (!title.trim()) {
      message.error('请输入任务标题');
      return;
    }

    setLoading(true);
    try {
      const trimmedWorkspace = workspace.trim() || null;

      if (isEditMode && todo) {
        // workspace 保存逻辑：
        // 1. 用户主动清空 workspace → 保存 null
        // 2. 输入了路径且在目录列表中命中 → 保存该路径
        // 3. 输入了路径但目录列表为空/未命中，且编辑模式下原值与当前一致 → 保留原值（避免目录加载失败时误清空）
        // 4. 其他情况（新建时输入了不存在的路径）→ 保存 null
        // 如果用户输入了路径但不在下拉列表中，不自动创建（name 必填约束），让用户使用快速新增功能
        const originalWorkspace = (todo.workspace || '').trim();
        const isKnownWorkspace = !!trimmedWorkspace && projectDirectories.some(d => d.path === trimmedWorkspace);
        const workspaceToSave = !trimmedWorkspace
          ? null
          : isKnownWorkspace || (isEditMode && trimmedWorkspace === originalWorkspace)
            ? trimmedWorkspace
            : null;

        await db.updateTodo(
          todo.id, title.trim(), prompt.trim(), todo.status,
          executor, schedulerEnabled, schedulerConfig || null,
          workspaceToSave, worktreeEnabled,
          hooks, acceptanceCriteria || null,
        );
        await db.updateScheduler(todo.id, schedulerEnabled, schedulerConfig || null);
        await db.updateTodoTags(todo.id, selectedTags);
        message.success('任务已更新');
      } else {
        const newTodo = await db.createTodo(title.trim(), prompt.trim(), selectedTags, hooks, acceptanceCriteria || undefined);

        // 创建模式：只在路径存在于目录列表时才设置 workspace，否则为 null（避免创建无名项目）
        const workspaceToSave = trimmedWorkspace && projectDirectories.some(d => d.path === trimmedWorkspace) ? trimmedWorkspace : null;

        if (workspaceToSave || schedulerEnabled || executor !== 'claudecode' || worktreeEnabled) {
          await db.updateTodo(
            newTodo.id, newTodo.title, newTodo.prompt, newTodo.status,
            executor, schedulerEnabled, schedulerConfig || null,
            workspaceToSave, worktreeEnabled,
            hooks, acceptanceCriteria || null,
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

  // 把项目目录拍平成 AutoComplete 的可选项：value 仍存路径（与后端 workspace 字段兼容），
  // label 同时展示"项目名称（路径）"，保证用户能看到项目维度的名字
  const workspaceOptions = useMemo(
    () =>
      projectDirectories.map(d => ({
        value: d.path,
        label: d.name ? `${d.name}（${d.path}）` : d.path,
      })),
    [projectDirectories]
  );

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
              项目目录
            </div>
            {projectDirectories.length === 0 ? (
              // 没有可用项目目录时给出空态提示，避免用户面对一个无选项的下拉茫然
              <div
                style={{
                  padding: 16,
                  border: '1px dashed var(--color-border)',
                  borderRadius: 6,
                  background: 'var(--color-bg)',
                }}
              >
                <Empty
                  image={Empty.PRESENTED_IMAGE_SIMPLE}
                  description={
                    <div style={{ color: 'var(--color-text-secondary)' }}>
                      尚未配置任何项目目录，请先创建后再选择
                    </div>
                  }
                  style={{ margin: 0 }}
                >
                  <Button
                    type="primary"
                    icon={<PlusOutlined />}
                    onClick={() => setQuickAddOpen(true)}
                  >
                    快速新增项目目录
                  </Button>
                </Empty>
              </div>
            ) : (
              // 有项目目录时，下拉里展示的是项目名称（value 仍是路径，与后端模型兼容）
              <Space.Compact style={{ width: '100%' }}>
                <AutoComplete
                  value={workspace}
                  onChange={(value) => setWorkspace(value)}
                  options={workspaceOptions}
                  placeholder="选择项目目录或手动输入路径"
                  style={{ flex: 1 }}
                  filterOption={(input, option) =>
                    (option?.label as string)?.toLowerCase().includes(input.toLowerCase())
                  }
                />
                <Button
                  icon={<PlusOutlined />}
                  onClick={() => setQuickAddOpen(true)}
                  title="快速新增项目目录"
                  aria-label="快速新增项目目录"
                />
              </Space.Compact>
            )}
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

          {/* 验收标准 */}
          <div style={{ marginBottom: 16 }}>
            <div style={{ marginBottom: 8, fontWeight: 600, fontSize: 14 }}>验收标准</div>
            <Input.TextArea
              value={acceptanceCriteria}
              onChange={e => setAcceptanceCriteria(e.target.value)}
              placeholder="描述完成该任务需要满足的条件..."
              rows={3}
              style={{ resize: 'vertical' }}
            />
          </div>

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

      {/* 快速新增项目目录：与抽屉同期挂载，避免切走 drawer 后弹窗无法关闭 */}
      <Modal
        title="快速新增项目目录"
        open={quickAddOpen}
        onCancel={() => {
          if (quickAddSubmitting) return;
          setQuickAddOpen(false);
          quickAddForm.resetFields();
        }}
        onOk={handleQuickAddProjectDirectory}
        confirmLoading={quickAddSubmitting}
        okText="保存并使用"
        cancelText="取消"
        destroyOnClose
        maskClosable={!quickAddSubmitting}
      >
        <Form form={quickAddForm} layout="vertical" preserve={false}>
          <Form.Item
            label="项目名称"
            name="name"
            // 名称必填：在 Todo 列表分组展示时是主标识
            rules={[{ required: true, message: '请输入项目名称' }]}
          >
            <Input placeholder="例如：ntd 官网重构" autoFocus />
          </Form.Item>
          <Form.Item
            label="目录路径"
            name="path"
            rules={[{ required: true, message: '请输入目录路径' }]}
          >
            <Input placeholder="例如：/Users/me/projects/ntd-site" />
          </Form.Item>
        </Form>
      </Modal>
    </Drawer>
  );
}
