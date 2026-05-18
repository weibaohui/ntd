import { useState, useEffect, useMemo } from 'react';
import { Drawer, Input, Button, App, AutoComplete, Divider, Switch, Tooltip, Tag, Empty, Spin } from 'antd';
import { CheckOutlined, FolderOutlined, ClockCircleOutlined, FileTextOutlined, ThunderboltOutlined, RightOutlined } from '@ant-design/icons';
import { Cron } from 'react-js-cron';
import 'react-js-cron/dist/styles.css';
import * as db from '../utils/database';
import type { ProjectDirectory } from '../utils/database';
import type { Todo, ExecutorConfig, ExecutorOption, SkillMeta, ExecutorSkills } from '../types';
import { CRON_ZH_LOCALE, cronTo5, cronTo6 } from '../utils/cron';
import { EXECUTORS, executorConfigToOption, getExecutorColor } from '../types';
import { TagCheckCardGroup } from './TagCheckCard';
import { CronPresetSelect } from './CronPresetSelect';
import { MdEditor } from './MdEditor';

interface TodoDrawerProps {
  open: boolean;
  todo: Todo | null; // null = create mode, Todo = edit mode
  tags: Array<{ id: number; name: string; color: string }>;
  onClose: () => void;
  onSaved: (todo?: Todo) => void; // callback after save
}

const DEFAULT_CRON = '0 */10 * * * *';

const PROMPT_PARAMS = [
  { key: '{{content}}', label: 'content', desc: '消息内容（已清理格式）' },
  { key: '{{message}}', label: 'message', desc: '原始消息文本' },
  { key: '{{raw_message}}', label: 'raw_message', desc: '未处理的原始消息' },
  { key: '{{slash_command}}', label: 'slash_command', desc: '斜杠命令内容' },
];

export function TodoDrawer({ open, todo, tags, onClose, onSaved }: TodoDrawerProps) {
  const { message } = App.useApp();
  const isEditMode = todo !== null;

  // Basic info
  const [title, setTitle] = useState('');
  const [prompt, setPrompt] = useState('');
  const [selectedTags, setSelectedTags] = useState<number[]>([]);

  // Executor & workspace
  const [executor, setExecutor] = useState<string>('claudecode');
  const [workspace, setWorkspace] = useState<string>('');
  const [worktreeEnabled, setWorktreeEnabled] = useState(false);
  const [executorOptions, setExecutorOptions] = useState<ExecutorOption[]>(EXECUTORS);
  const [projectDirectories, setProjectDirectories] = useState<ProjectDirectory[]>([]);

  // Skills
  const [allExecutorSkills, setAllExecutorSkills] = useState<ExecutorSkills[]>([]);
  const [skillsLoading, setSkillsLoading] = useState(false);
  const [skillsExpanded, setSkillsExpanded] = useState(false);

  // Scheduler
  const [schedulerEnabled, setSchedulerEnabled] = useState(false);
  const [schedulerConfig, setSchedulerConfig] = useState<string>('');

  // Loading states
  const [loading, setLoading] = useState(false);

  // Filter skills for current executor
  const currentSkills = useMemo(() => {
    const found = allExecutorSkills.find(e => e.executor === executor);
    return found?.skills || [];
  }, [executor, allExecutorSkills]);

  // Initialize data when drawer opens
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

      // Load skills list
      setSkillsLoading(true);
      db.getSkillsList()
        .then((data) => setAllExecutorSkills(data))
        .catch(() => {})
        .finally(() => setSkillsLoading(false));
    }
  }, [open]);

  // Reset or populate form when todo changes
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
      } else {
        // Create mode - reset
        setTitle('');
        setPrompt('');
        setSelectedTags([]);
        setExecutor('claudecode');
        setWorkspace('');
        setWorktreeEnabled(false);
        setSchedulerEnabled(false);
        setSchedulerConfig('');
      }
    }
  }, [open, todo]);

  const handleSkillClick = (skill: SkillMeta) => {
    const skillRef = `/${skill.name}`;
    setPrompt(prev => {
      if (!prev.trim()) return skillRef;
      // If prompt already ends with newline, just append
      if (prev.endsWith('\n')) return prev + skillRef;
      return prev + '\n' + skillRef;
    });
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
        // Update existing todo
        if (trimmedWorkspace) {
          const exists = projectDirectories.some(d => d.path === trimmedWorkspace);
          if (!exists) {
            try {
              await db.createProjectDirectory(trimmedWorkspace);
            } catch {
              // Ignore
            }
          }
        }

        await db.updateTodo(
          todo.id,
          title.trim(),
          prompt.trim(),
          todo.status,
          executor,
          schedulerEnabled,
          schedulerConfig || null,
          trimmedWorkspace,
          worktreeEnabled,
        );
        await db.updateScheduler(todo.id, schedulerEnabled, schedulerConfig || null);
        await db.updateTodoTags(todo.id, selectedTags);
        message.success('任务已更新');
      } else {
        // Create new todo
        const newTodo = await db.createTodo(title.trim(), prompt.trim(), selectedTags);

        // If settings are configured, update them
        if (trimmedWorkspace || schedulerEnabled || executor !== 'claudecode' || worktreeEnabled) {
          if (trimmedWorkspace) {
            const exists = projectDirectories.some(d => d.path === trimmedWorkspace);
            if (!exists) {
              try {
                await db.createProjectDirectory(trimmedWorkspace);
              } catch {
                // Ignore
              }
            }
          }
          await db.updateTodo(
            newTodo.id,
            newTodo.title,
            newTodo.prompt,
            newTodo.status,
            executor,
            schedulerEnabled,
            schedulerConfig || null,
            trimmedWorkspace,
            worktreeEnabled,
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
      styles={{
        body: { padding: 0 },
      }}
      extra={
        <Button type="primary" loading={loading} onClick={handleSave}>
          {isEditMode ? '保存' : '创建'}
        </Button>
      }
    >
      <div style={{
        display: 'flex',
        flexDirection: 'column',
        height: '100%',
        background: 'var(--color-bg-elevated)',
      }}>
        {/* Header with title input */}
        <div style={{ padding: '16px 20px', borderBottom: '1px solid var(--color-border-light)' }}>
          <Input
            value={title}
            onChange={e => setTitle(e.target.value)}
            placeholder="任务标题"
            style={{
              fontSize: 16,
              fontWeight: 600,
              padding: '8px 12px',
            }}
          />
        </div>

        {/* Scrollable content */}
        <div style={{ flex: 1, overflow: 'auto', padding: '16px 20px' }}>
          {/* Executor Selection */}
          <div style={{ marginBottom: 16 }}>
            <div style={{ marginBottom: 10, fontWeight: 600, fontSize: 14 }}>执行器</div>
            <div style={{ display: 'flex', flexWrap: 'wrap', gap: 10 }}>
              {executorOptions.map((opt) => {
                const selected = executor === opt.value;
                return (
                  <div
                    key={opt.value}
                    onClick={() => setExecutor(opt.value)}
                    role="button"
                    tabIndex={0}
                    onKeyDown={(e) => {
                      if (e.key === 'Enter' || e.key === ' ') {
                        e.preventDefault();
                        setExecutor(opt.value);
                      }
                    }}
                    style={{
                      display: 'flex',
                      alignItems: 'center',
                      gap: 8,
                      padding: '10px 14px',
                      borderRadius: 10,
                      border: `2px solid ${selected ? opt.color : 'var(--color-border-secondary)'}`,
                      background: selected ? `${opt.color}10` : 'var(--color-bg-elevated)',
                      cursor: 'pointer',
                      transition: 'all 0.2s ease',
                      flex: '1 1 calc(50% - 10px)',
                      minWidth: 120,
                    }}
                    onMouseEnter={(e) => {
                      if (!selected) {
                        (e.currentTarget as HTMLDivElement).style.borderColor = `${opt.color}60`;
                        (e.currentTarget as HTMLDivElement).style.background = `${opt.color}08`;
                      }
                    }}
                    onMouseLeave={(e) => {
                      if (!selected) {
                        (e.currentTarget as HTMLDivElement).style.borderColor = 'var(--color-border-secondary)';
                        (e.currentTarget as HTMLDivElement).style.background = 'var(--color-bg-elevated)';
                      }
                    }}
                  >
                    <span style={{ fontSize: 16, lineHeight: 1 }}>{opt.icon}</span>
                    <span style={{
                      fontSize: 14,
                      fontWeight: 600,
                      color: selected ? opt.color : 'var(--color-text)',
                      flex: 1,
                    }}>
                      {opt.label}
                    </span>
                    {selected && (
                      <span style={{
                        width: 18,
                        height: 18,
                        borderRadius: '50%',
                        backgroundColor: opt.color,
                        display: 'flex',
                        alignItems: 'center',
                        justifyContent: 'center',
                        flexShrink: 0,
                      }}>
                        <CheckOutlined style={{ fontSize: 10, color: '#fff' }} />
                      </span>
                    )}
                  </div>
                );
              })}
            </div>
          </div>

          <Divider style={{ margin: '8px 0 16px' }} />

          {/* Tags */}
          {tags.length > 0 && (
            <>
              <div style={{ marginBottom: 16 }}>
                <div style={{ marginBottom: 10, fontWeight: 600, fontSize: 14 }}>标签</div>
                <TagCheckCardGroup
                  tags={tags}
                  value={selectedTags[0] || null}
                  onChange={(val) => setSelectedTags(val ? [val as number] : [])}
                />
              </div>
              <Divider style={{ margin: '8px 0 16px' }} />
            </>
          )}

          {/* Prompt Editor */}
          <div style={{ marginBottom: 16 }}>
            <div style={{ marginBottom: 10, fontWeight: 600, fontSize: 14 }}>
              <FileTextOutlined style={{ color: 'var(--color-primary)', marginRight: 6 }} />
              Prompt
            </div>
            <MdEditor
              value={prompt}
              onChange={setPrompt}
              height={200}
            />
            {/* Prompt parameter hints */}
            <div style={{
              marginTop: 8,
              display: 'flex',
              flexWrap: 'wrap',
              gap: 6,
              alignItems: 'center',
            }}>
              <span style={{ fontSize: 12, color: 'var(--color-text-tertiary)', marginRight: 2 }}>可用参数:</span>
              {PROMPT_PARAMS.map(p => (
                <Tooltip key={p.key} title={p.desc}>
                  <code
                    onClick={() => {
                      setPrompt(prev => prev + p.key);
                    }}
                    style={{
                      fontSize: 11,
                      padding: '1px 6px',
                      borderRadius: 4,
                      background: 'var(--color-fill-quaternary)',
                      border: '1px solid var(--color-border-secondary)',
                      cursor: 'pointer',
                      color: 'var(--color-text-secondary)',
                      transition: 'all 0.2s',
                    }}
                    onMouseEnter={(e) => {
                      (e.currentTarget as HTMLElement).style.borderColor = 'var(--color-primary)';
                      (e.currentTarget as HTMLElement).style.color = 'var(--color-primary)';
                    }}
                    onMouseLeave={(e) => {
                      (e.currentTarget as HTMLElement).style.borderColor = 'var(--color-border-secondary)';
                      (e.currentTarget as HTMLElement).style.color = 'var(--color-text-secondary)';
                    }}
                  >
                    {p.key}
                  </code>
                </Tooltip>
              ))}
            </div>
          </div>

          {/* Skills Card List */}
          {currentSkills.length > 0 && (
            <div style={{ marginBottom: 16 }}>
              <div
                onClick={() => setSkillsExpanded(prev => !prev)}
                role="button"
                tabIndex={0}
                onKeyDown={(e) => {
                  if (e.key === 'Enter' || e.key === ' ') {
                    e.preventDefault();
                    setSkillsExpanded(prev => !prev);
                  }
                }}
                style={{
                  marginBottom: skillsExpanded ? 10 : 0,
                  fontWeight: 600,
                  fontSize: 14,
                  cursor: 'pointer',
                  display: 'flex',
                  alignItems: 'center',
                  userSelect: 'none',
                }}
              >
                <RightOutlined style={{
                  color: executorColor,
                  fontSize: 10,
                  marginRight: 6,
                  transition: 'transform 0.2s',
                  transform: skillsExpanded ? 'rotate(90deg)' : 'rotate(0deg)',
                }} />
                <ThunderboltOutlined style={{ color: executorColor, marginRight: 6 }} />
                Skills
                <span style={{ fontWeight: 400, fontSize: 12, color: 'var(--color-text-tertiary)', marginLeft: 8 }}>
                  {currentSkills.length} 个可用
                </span>
              </div>
              {skillsExpanded && (
                <div style={{
                  display: 'grid',
                  gridTemplateColumns: 'repeat(2, 1fr)',
                  gap: 10,
                }}>
                  {currentSkills.map(skill => (
                  <div
                    key={skill.name}
                    onClick={() => handleSkillClick(skill)}
                    role="button"
                    tabIndex={0}
                    onKeyDown={(e) => {
                      if (e.key === 'Enter' || e.key === ' ') {
                        e.preventDefault();
                        handleSkillClick(skill);
                      }
                    }}
                    style={{
                      padding: '10px 12px',
                      borderRadius: 8,
                      border: '1px solid var(--color-border-secondary)',
                      background: 'var(--color-bg-elevated)',
                      cursor: 'pointer',
                      transition: 'all 0.2s ease',
                      overflow: 'hidden',
                    }}
                    onMouseEnter={(e) => {
                      (e.currentTarget as HTMLDivElement).style.borderColor = executorColor;
                      (e.currentTarget as HTMLDivElement).style.background = `${executorColor}08`;
                    }}
                    onMouseLeave={(e) => {
                      (e.currentTarget as HTMLDivElement).style.borderColor = 'var(--color-border-secondary)';
                      (e.currentTarget as HTMLDivElement).style.background = 'var(--color-bg-elevated)';
                    }}
                  >
                    <div style={{
                      fontSize: 13,
                      fontWeight: 600,
                      color: 'var(--color-text)',
                      overflow: 'hidden',
                      textOverflow: 'ellipsis',
                      whiteSpace: 'nowrap',
                    }}>
                      {skill.name}
                    </div>
                    {skill.description && (
                      <div style={{
                        fontSize: 11,
                        color: 'var(--color-text-tertiary)',
                        marginTop: 4,
                        overflow: 'hidden',
                        textOverflow: 'ellipsis',
                        whiteSpace: 'nowrap',
                      }}>
                        {skill.description}
                      </div>
                    )}
                    <div style={{
                      display: 'flex',
                      flexWrap: 'wrap',
                      gap: 4,
                      marginTop: 6,
                      alignItems: 'center',
                    }}>
                      {skill.version && (
                        <Tag style={{ fontSize: 10, lineHeight: '16px', padding: '0 4px', margin: 0 }} color="blue">v{skill.version}</Tag>
                      )}
                      {skill.author && (
                        <span style={{ fontSize: 10, color: 'var(--color-text-quaternary)' }}>{skill.author}</span>
                      )}
                      {skill.file_count > 0 && (
                        <span style={{ fontSize: 10, color: 'var(--color-text-quaternary)', marginLeft: 'auto' }}>
                          {skill.file_count} 文件
                        </span>
                      )}
                    </div>
                  </div>
                ))}
              </div>
              )}
            </div>
          )}

          {skillsLoading && currentSkills.length === 0 && (
            <div style={{ textAlign: 'center', padding: 16 }}>
              <Spin size="small" />
            </div>
          )}

          {!skillsLoading && currentSkills.length === 0 && allExecutorSkills.length > 0 && (
            <div style={{ marginBottom: 16 }}>
              <Empty
                image={Empty.PRESENTED_IMAGE_SIMPLE}
                description="当前执行器暂无 Skills"
                style={{ margin: 0 }}
              />
            </div>
          )}

          <Divider style={{ margin: '8px 0 16px' }} />

          {/* Workspace */}
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

          {/* Worktree Switch */}
          {(executor === 'claudecode' || executor === 'hermes') && (
            <div style={{ marginBottom: 16 }}>
              <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
                <div style={{ fontWeight: 600, fontSize: 14 }}>
                  Git Worktree
                </div>
                <Switch
                  checked={worktreeEnabled}
                  onChange={(checked) => setWorktreeEnabled(checked)}
                  disabled={!workspace}
                />
              </div>
              {!workspace && (
                <div style={{ fontSize: 12, color: 'var(--color-text-tertiary)', marginTop: 4 }}>
                  请先设置工作目录
                </div>
              )}
            </div>
          )}

          <Divider style={{ margin: '8px 0 16px' }} />

          {/* Scheduler */}
          <div>
            <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 12 }}>
              <div style={{ fontWeight: 600, fontSize: 14 }}>
                <ClockCircleOutlined style={{ color: 'var(--color-primary)', marginRight: 6 }} />
                定时调度
              </div>
              <Switch
                checked={schedulerEnabled}
                onChange={(checked) => {
                  setSchedulerEnabled(checked);
                  if (checked && !schedulerConfig) {
                    setSchedulerConfig(DEFAULT_CRON);
                  }
                }}
              />
            </div>

            {schedulerEnabled && (
              <div style={{ marginTop: 12 }}>
                <CronPresetSelect
                  value={schedulerConfig || DEFAULT_CRON}
                  onChange={(val) => setSchedulerConfig(val)}
                />
                <div style={{ marginTop: 12 }}>
                  <Cron
                    value={cronTo5(schedulerConfig || DEFAULT_CRON)}
                    setValue={(val: string) => setSchedulerConfig(cronTo6(val))}
                    locale={CRON_ZH_LOCALE}
                    defaultPeriod="hour"
                    humanizeLabels
                    allowClear={false}
                  />
                </div>
              </div>
            )}

            {todo?.scheduler_config && (
              <div style={{ marginTop: 8, fontSize: 12, color: 'var(--color-text-tertiary)' }}>
                当前配置: <code>{todo.scheduler_config}</code>
              </div>
            )}
          </div>
        </div>
      </div>
    </Drawer>
  );
}
