import { useState, useEffect } from 'react';
import { Button, Input, Empty, Spin, Switch, message, Tooltip, Typography, Card, Dropdown } from 'antd';
import { PlusOutlined, FolderOutlined, RobotOutlined, SettingOutlined, EditOutlined, DeleteOutlined, MessageOutlined, MoreOutlined } from '@ant-design/icons';
import { PageCard } from '@/components/common/PageCard';
import * as db from '@/utils/database';
import type { ProjectDirectory, AgentBot } from '@/utils/database';
import { WorkspaceMessageConfigPage } from './workspace/WorkspaceMessageConfigPage';
import { WorkspaceLoopConfigPage } from './workspace/WorkspaceLoopConfigPage';

export function ProjectDirectoriesPanel() {
  // 项目目录列表；按 path 升序，保持稳定可读
  const [projectDirectories, setProjectDirectories] = useState<ProjectDirectory[]>([]);
  const [projectDirsLoading, setProjectDirsLoading] = useState(false);
  // 新增表单的路径与名称：均为必填项，名称是 Todo 侧按"项目"识别目录的唯一 key
  const [newDirPath, setNewDirPath] = useState('');
  const [newDirName, setNewDirName] = useState('');
  const [addingDir, setAddingDir] = useState(false);
  const [editingDirId, setEditingDirId] = useState<number | null>(null);
  const [editingDirName, setEditingDirName] = useState('');
  // 智能体列表，用于统计每个工作区的绑定数量
  const [agentBots, setAgentBots] = useState<AgentBot[]>([]);
  // 选中的工作空间和页面类型
  const [selectedWorkspace, setSelectedWorkspace] = useState<ProjectDirectory | null>(null);
  const [selectedPageType, setSelectedPageType] = useState<'message' | 'loop' | null>(null);

  // 每次进入页面都重新拉取一次，确保用户在其他地方新增/删除后能立刻看到
  const loadProjectDirectories = () => {
    setProjectDirsLoading(true);
    db.getProjectDirectories()
      .then(setProjectDirectories)
      .catch((err: any) => message.error('加载项目目录失败: ' + (err?.message || String(err))))
      .finally(() => setProjectDirsLoading(false));
  };

  /**
   * 加载智能体列表，用于统计每个工作区的绑定数量
   */
  const loadAgentBots = () => {
    db.getAgentBots()
      .then(setAgentBots)
      .catch((err: any) => message.error('加载智能体列表失败: ' + (err?.message || String(err))));
  };

  /**
   * 获取指定工作区的智能体数量
   */
  const getWorkspaceBotCount = (workspaceId: number) => {
    return agentBots.filter(bot => bot.workspace_id === workspaceId).length;
  };

  useEffect(() => {
    loadProjectDirectories();
    loadAgentBots();
    // 监听其他组件新增目录的事件，及时刷新列表
    const reload = () => loadProjectDirectories();
    window.addEventListener('projectDirectoryAdded', reload);
    return () => window.removeEventListener('projectDirectoryAdded', reload);
  }, []);

  const handleAddProjectDirectory = async () => {
    const path = newDirPath.trim();
    const name = newDirName.trim();
    // 名称与路径都为必填：项目目录是 Todo 按"项目"维度分组的依据，
    // 任意一项缺失都会让 Todo 侧无法定位到具体项目分组
    if (!path) {
      message.error('请输入目录路径');
      return;
    }
    if (!name) {
      message.error('请输入项目名称');
      return;
    }
    setAddingDir(true);
    try {
      const dir = await db.createProjectDirectory(path, name);
      setProjectDirectories(prev => [...prev.filter(d => d.id !== dir.id), dir].sort((a, b) => a.path.localeCompare(b.path)));
      setNewDirPath('');
      setNewDirName('');
      message.success('添加成功');
    } catch (err: any) {
      message.error('添加失败: ' + (err?.message || String(err)));
    } finally {
      setAddingDir(false);
    }
  };

  const handleUpdateProjectDirectoryName = async (id: number) => {
    const name = editingDirName.trim();
    if (!name) {
      message.error('请输入项目名称');
      return;
    }
    try {
      await db.updateProjectDirectory(id, name);
      setProjectDirectories(prev => prev.map(d => d.id === id ? { ...d, name } : d));
      setEditingDirId(null);
      setEditingDirName('');
      message.success('更新成功');
    } catch (err: any) {
      message.error('更新失败: ' + (err?.message || String(err)));
    }
  };

  /// issue #643: 切换 worktree 开关。state 乐观更新 + 失败回滚，避免用户点完后看到
  /// 状态没反应误以为系统卡住。
  const handleToggleWorktree = async (id: number, flag: 'gitWorktreeEnabled' | 'autoCleanup', next: boolean) => {
    const target = projectDirectories.find(d => d.id === id);
    if (!target) return;
    // auto_cleanup 强依赖 git_worktree_enabled 开启：开 auto 但关 worktree 是废组合，
    // 这里在前端先拦一道，避免后端拒绝请求时还走一次无谓的 HTTP。
    if (flag === 'autoCleanup' && next && !target.git_worktree_enabled) {
      message.warning('请先开启"启用 Git Worktree"');
      return;
    }
    // 计算乐观更新与请求体：键名要用 snake_case（与后端/类型定义一致），
    // 之前用 `[flag]: next` 直接挂 camelCase 键会导致 UI 与 API 不一致。
    // 当关闭 git_worktree_enabled 时联动把 auto_cleanup 复位为 false：
    // 因为 auto_cleanup 在 git worktree 关闭后已无意义，留着只会让 UI 显示一个永远
    // 不会触发的「自动清理」勾，给人误导。
    const nextGit = flag === 'gitWorktreeEnabled' ? next : (target.git_worktree_enabled ?? false);
    const nextAuto = flag === 'autoCleanup' ? next : (target.auto_cleanup ?? false);
    const optimistic: ProjectDirectory = {
      ...target,
      git_worktree_enabled: nextGit,
      // 仅在「关闭 git_worktree_enabled」时把 auto_cleanup 拉回 false，单独切 auto_cleanup 不联动 git
      auto_cleanup: nextGit ? nextAuto : false,
    };
    setProjectDirectories(prev => prev.map(d => d.id === id ? optimistic : d));
    const previous = target;
    try {
      await db.updateProjectDirectory(id, target.name ?? '', {
        gitWorktreeEnabled: nextGit,
        autoCleanup: nextGit ? nextAuto : false,
      });
    } catch (err: any) {
      // 失败回滚到之前的值，并提示用户
      setProjectDirectories(prev => prev.map(d => d.id === id ? previous : d));
      message.error('更新失败: ' + (err?.message || String(err)));
    }
  };

  const handleDeleteProjectDirectory = async (id: number) => {
    try {
      await db.deleteProjectDirectory(id);
      setProjectDirectories(prev => prev.filter(d => d.id !== id));
      message.success('删除成功');
    } catch (err: any) {
      message.error('删除失败: ' + (err?.message || String(err)));
    }
  };

  // 选中工作空间，显示对应配置页
  if (selectedWorkspace && selectedPageType === 'message') {
    return (
      <WorkspaceMessageConfigPage
        workspace={selectedWorkspace}
        onBack={() => { setSelectedWorkspace(null); setSelectedPageType(null); loadAgentBots(); }}
      />
    );
  }
  if (selectedWorkspace && selectedPageType === 'loop') {
    return (
      <WorkspaceLoopConfigPage
        workspace={selectedWorkspace}
        onBack={() => { setSelectedWorkspace(null); setSelectedPageType(null); }}
      />
    );
  }

  return (
    <PageCard icon={<FolderOutlined />} title="工作空间">
      <div style={{ maxWidth: 800 }}>
        <Spin spinning={projectDirsLoading}>
          {/* 新建工作空间区域 */}
          <Card size="small" style={{ marginBottom: 24, borderRadius: 12 }}>
            <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 12 }}>
              <PlusOutlined style={{ color: 'var(--color-primary)', fontSize: 16 }} />
              <span style={{ fontWeight: 600, fontSize: 15 }}>新建工作空间</span>
            </div>
            <div style={{ fontSize: 12, color: 'var(--color-text-secondary)', marginBottom: 16 }}>
              添加本地目录作为工作空间，按项目维度组织和管理事项。
            </div>
            <div style={{ display: 'flex', gap: 12, alignItems: 'center' }}>
              <Input
                value={newDirName}
                onChange={(e) => setNewDirName(e.target.value)}
                placeholder="名称"
                style={{ width: 180 }}
                onPressEnter={handleAddProjectDirectory}
                size="large"
              />
              <Input
                value={newDirPath}
                onChange={(e) => setNewDirPath(e.target.value)}
                placeholder="路径"
                style={{ flex: 1 }}
                onPressEnter={handleAddProjectDirectory}
                size="large"
              />
              <Button
                type="primary"
                icon={<PlusOutlined />}
                loading={addingDir}
                onClick={handleAddProjectDirectory}
                size="large"
              >
                添加
              </Button>
            </div>
          </Card>

          {/* 工作空间列表 */}
          {projectDirectories.length === 0 ? (
            <Empty description="暂无工作空间" image={Empty.PRESENTED_IMAGE_SIMPLE} />
          ) : (
            <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
              {projectDirectories.map((dir) => (
                <Card
                  key={dir.id}
                  size="small"
                  style={{
                    borderRadius: 12,
                    transition: 'all 0.2s',
                    border: '1px solid var(--color-border-light)',
                  }}
                >
                  <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
                    {/* 左侧：图标 + 名称 + 路径 + 智能体数量 */}
                    <div style={{ display: 'flex', alignItems: 'center', gap: 12, flex: 1, minWidth: 0 }}>
                      <div
                        style={{
                          width: 40,
                          height: 40,
                          borderRadius: 10,
                          background: 'linear-gradient(135deg, #1890ff 0%, #096dd9 100%)',
                          display: 'flex',
                          alignItems: 'center',
                          justifyContent: 'center',
                          flexShrink: 0,
                        }}
                      >
                        <FolderOutlined style={{ fontSize: 20, color: '#fff' }} />
                      </div>
                      <div style={{ flex: 1, minWidth: 0 }}>
                        {editingDirId === dir.id ? (
                          <div style={{ display: 'flex', gap: 8, alignItems: 'center' }}>
                            <Input
                              value={editingDirName}
                              onChange={(e) => setEditingDirName(e.target.value)}
                              placeholder="输入名称"
                              size="small"
                              style={{ width: 180 }}
                              onPressEnter={() => handleUpdateProjectDirectoryName(dir.id)}
                              autoFocus
                            />
                            <Button size="small" type="primary" onClick={() => handleUpdateProjectDirectoryName(dir.id)}>保存</Button>
                            <Button size="small" onClick={() => { setEditingDirId(null); setEditingDirName(''); }}>取消</Button>
                          </div>
                        ) : (
                          <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                            <span style={{
                              fontSize: 15,
                              fontWeight: 600,
                              color: 'var(--color-text)',
                            }}>
                              {dir.name || <span style={{ color: 'var(--color-warning)' }}>未命名</span>}
                            </span>
                            {/* 绑定消息智能体数量，可点击进入消息配置页 */}
                            <Typography.Link
                              type="secondary"
                              style={{
                                fontSize: 12,
                                display: 'inline-flex',
                                alignItems: 'center',
                                gap: 4,
                                padding: '2px 8px',
                                background: 'var(--color-bg)',
                                borderRadius: 4,
                              }}
                              onClick={() => { setSelectedWorkspace(dir); setSelectedPageType('message'); }}
                            >
                              <RobotOutlined />
                              {getWorkspaceBotCount(dir.id)}
                            </Typography.Link>
                          </div>
                        )}
                        <div style={{
                          fontSize: 13,
                          color: 'var(--color-text-secondary)',
                          marginTop: 2,
                          overflow: 'hidden',
                          textOverflow: 'ellipsis',
                          whiteSpace: 'nowrap',
                          fontFamily: 'monospace',
                        }}>
                          {dir.path}
                        </div>
                      </div>
                    </div>

                    {/* 右侧：操作区域 */}
                    <div style={{ display: 'flex', alignItems: 'center', gap: 8, flexShrink: 0 }}>
                      {/* 快捷配置按钮 */}
                      <Button
                        size="small"
                        icon={<MessageOutlined />}
                        onClick={() => { setSelectedWorkspace(dir); setSelectedPageType('message'); }}
                      >
                        消息配置
                      </Button>
                      <Button
                        size="small"
                        icon={<SettingOutlined />}
                        onClick={() => { setSelectedWorkspace(dir); setSelectedPageType('loop'); }}
                      >
                        环路配置
                      </Button>
                      {/* 更多操作菜单 */}
                      <Dropdown
                        menu={{
                          items: [
                            {
                              key: 'edit',
                              icon: <EditOutlined />,
                              label: '编辑',
                              onClick: () => { setEditingDirId(dir.id); setEditingDirName(dir.name || ''); },
                            },
                            {
                              key: 'delete',
                              icon: <DeleteOutlined />,
                              label: '删除',
                              danger: true,
                            },
                          ],
                          onClick: ({ key }) => {
                            if (key === 'delete') {
                              handleDeleteProjectDirectory(dir.id);
                            }
                          },
                        }}
                      >
                        <Button type="text" size="small" icon={<MoreOutlined />} />
                      </Dropdown>
                    </div>
                  </div>

                  {/* 底部：功能开关区 */}
                  <div
                    style={{
                      display: 'flex',
                      gap: 24,
                      marginTop: 16,
                      paddingTop: 12,
                      borderTop: '1px solid var(--color-border-light)',
                      flexWrap: 'wrap',
                    }}
                  >
                    <Tooltip title="执行事项时自动创建 git worktree，保持工作区干净">
                      <span style={{ display: 'inline-flex', alignItems: 'center', gap: 8, cursor: 'pointer' }}>
                        <Switch
                          size="small"
                          checked={!!dir.git_worktree_enabled}
                          onChange={(v) => handleToggleWorktree(dir.id, 'gitWorktreeEnabled', v)}
                        />
                        <span style={{ fontSize: 13, color: 'var(--color-text-secondary)' }}>Git Worktree</span>
                      </span>
                    </Tooltip>
                    <Tooltip title="执行结束后自动清理 worktree 目录（需先开启 Worktree）">
                      <span style={{ display: 'inline-flex', alignItems: 'center', gap: 8, cursor: 'pointer' }}>
                        <Switch
                          size="small"
                          checked={!!dir.auto_cleanup}
                          disabled={!dir.git_worktree_enabled}
                          onChange={(v) => handleToggleWorktree(dir.id, 'autoCleanup', v)}
                        />
                        <span style={{
                          fontSize: 13,
                          color: !dir.git_worktree_enabled ? 'var(--color-text-tertiary)' : 'var(--color-text-secondary)',
                        }}>
                          自动清理
                        </span>
                      </span>
                    </Tooltip>
                  </div>
                </Card>
              ))}
            </div>
          )}
        </Spin>
      </div>
    </PageCard>
  );
}
