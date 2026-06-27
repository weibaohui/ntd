import { useState, useEffect } from 'react';
import { Button, Popconfirm, Input, Space, Empty, Spin, Switch, message, Tooltip, Badge } from 'antd';
import { PlusOutlined, FolderOutlined, QuestionCircleOutlined, RobotOutlined } from '@ant-design/icons';
import { PageCard } from '@/components/common/PageCard';
import * as db from '@/utils/database';
import type { ProjectDirectory, AgentBot } from '@/utils/database';
import { WorkspaceDetailPage, BotDetailPage } from './workspace';

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
  // 进入工作空间详情页
  const [selectedWorkspace, setSelectedWorkspace] = useState<ProjectDirectory | null>(null);
  // 直接进入 Bot 详情页（消息智能体配置）
  const [selectedBotForDetail, setSelectedBotForDetail] = useState<AgentBot | null>(null);
  // 全部智能体列表（用于按 workspace 过滤显示数量）
  const [allBots, setAllBots] = useState<AgentBot[]>([]);

  // 每次进入页面都重新拉取一次，确保用户在其他地方新增/删除后能立刻看到
  const loadProjectDirectories = () => {
    setProjectDirsLoading(true);
    Promise.all([db.getProjectDirectories(), db.getAgentBots()])
      .then(([dirs, bots]) => {
        setProjectDirectories(dirs);
        setAllBots(bots);
      })
      .catch((err: any) => message.error('加载项目目录失败: ' + (err?.message || String(err))))
      .finally(() => setProjectDirsLoading(false));
  };

  useEffect(() => {
    loadProjectDirectories();
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

  // 获取指定工作空间绑定的智能体数量
  const getBotCount = (workspaceId: number) => allBots.filter(b => b.workspace_id === workspaceId).length;

  // 点击智能体数量，进入该工作空间第一个智能体的详情页
  const handleBotCountClick = (workspaceId: number) => {
    const botsInWorkspace = allBots.filter(b => b.workspace_id === workspaceId);
    if (botsInWorkspace.length > 0) {
      setSelectedBotForDetail(botsInWorkspace[0]);
    } else {
      message.info('该工作空间暂无绑定的消息智能体');
    }
  };

  // 选中 Bot，显示消息智能体配置页
  if (selectedBotForDetail) {
    return (
      <BotDetailPage
        bot={selectedBotForDetail}
        onBack={() => {
          setSelectedBotForDetail(null);
          // 刷新一下 bot 列表，确保数量准确
          db.getAgentBots().then(setAllBots).catch(() => {});
        }}
        onRefresh={() => {
          db.getAgentBots().then(setAllBots).catch(() => {});
        }}
      />
    );
  }

  // 选中工作空间，显示详情页（Loop 配置直接展示，不再有 tab 切换）
  if (selectedWorkspace) {
    return (
      <WorkspaceDetailPage
        workspace={selectedWorkspace}
        onBack={() => setSelectedWorkspace(null)}
      />
    );
  }

  return (
    <PageCard icon={<FolderOutlined />} title="工作空间">
      <div style={{ maxWidth: 760 }}>
        <Spin spinning={projectDirsLoading}>
          {/* 新建工作空间区域 */}
          <div
            style={{
              background: 'var(--color-bg)',
              border: '1px solid var(--color-border-light)',
              borderRadius: 10,
              padding: 16,
              marginBottom: 20,
            }}
          >
            <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 8 }}>
              <PlusOutlined style={{ color: 'var(--color-primary)', fontSize: 14 }} />
              <span style={{ fontWeight: 600, fontSize: 14 }}>新建工作空间</span>
            </div>
            <div style={{ fontSize: 12, color: 'var(--color-text-secondary)', marginBottom: 12 }}>
              添加本地目录作为工作空间，按项目维度组织和管理事项。
            </div>
            <div style={{ display: 'flex', gap: 8, alignItems: 'center' }}>
              <Input
                value={newDirName}
                onChange={(e) => setNewDirName(e.target.value)}
                placeholder="名称，如 my-app"
                style={{ flex: 1 }}
                onPressEnter={handleAddProjectDirectory}
              />
              <Input
                value={newDirPath}
                onChange={(e) => setNewDirPath(e.target.value)}
                placeholder="路径，如 /Users/name/projects/my-app"
                style={{ flex: 2 }}
                onPressEnter={handleAddProjectDirectory}
              />
              <Button
                type="primary"
                icon={<PlusOutlined />}
                loading={addingDir}
                onClick={handleAddProjectDirectory}
              >
                添加
              </Button>
            </div>
          </div>

          {/* 工作空间列表 */}
          {projectDirectories.length === 0 ? (
            <Empty description="暂无工作空间" image={Empty.PRESENTED_IMAGE_SIMPLE} />
          ) : (
            <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
              {projectDirectories.map((dir) => (
                <div
                  key={dir.id}
                  style={{
                    background: 'var(--color-bg)',
                    border: '1px solid var(--color-border-light)',
                    borderRadius: 10,
                    padding: 14,
                    transition: 'all 0.2s',
                  }}
                >
                  <div style={{ display: 'flex', alignItems: 'flex-start', gap: 12 }}>
                    {/* 左侧：图标 + 名称 + 路径 */}
                    <div style={{ flex: 1, minWidth: 0 }}>
                      <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 4 }}>
                        <FolderOutlined style={{ fontSize: 18, color: '#1890ff', flexShrink: 0 }} />
                        {editingDirId === dir.id ? (
                          <div style={{ display: 'flex', gap: 6, alignItems: 'center' }}>
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
                          <span style={{
                            fontSize: 15,
                            fontWeight: 600,
                            overflow: 'hidden',
                            textOverflow: 'ellipsis',
                            whiteSpace: 'nowrap',
                            color: 'var(--color-text)',
                          }}>
                            {dir.name || <span style={{ color: 'var(--color-warning)' }}>未命名</span>}
                          </span>
                        )}
                      </div>
                      <div style={{
                        fontSize: 12,
                        color: 'var(--color-text-secondary)',
                        paddingLeft: 26,
                        overflow: 'hidden',
                        textOverflow: 'ellipsis',
                        whiteSpace: 'nowrap',
                        fontFamily: 'monospace',
                      }}>
                        {dir.path}
                      </div>
                    </div>

                    {/* 右侧：操作按钮 */}
                    <Space size={4}>
                      {editingDirId !== dir.id && (
                        <Button
                          size="small"
                          onClick={() => { setEditingDirId(dir.id); setEditingDirName(dir.name || ''); }}
                        >
                          编辑
                        </Button>
                      )}
                      {/* 消息智能体数量badge，点击进入配置页 */}
                      <Tooltip title="点击进入消息智能体配置">
                        <Badge count={getBotCount(dir.id)} size="small" offset={[-2, 0]}>
                          <Button
                            type="primary"
                            size="small"
                            icon={<RobotOutlined />}
                            onClick={() => handleBotCountClick(dir.id)}
                          >
                            消息配置
                          </Button>
                        </Badge>
                      </Tooltip>
                      <Button
                        size="small"
                        onClick={() => setSelectedWorkspace(dir)}
                      >
                        Loop配置
                      </Button>
                      <Popconfirm
                        title="删除工作空间"
                        description={`确定要删除 "${dir.name || dir.path}" 吗？`}
                        onConfirm={() => handleDeleteProjectDirectory(dir.id)}
                      >
                        <Button size="small" danger>删除</Button>
                      </Popconfirm>
                    </Space>
                  </div>

                  {/* 底部：功能开关区 */}
                  <div
                    style={{
                      display: 'flex',
                      gap: 20,
                      marginTop: 12,
                      paddingTop: 12,
                      borderTop: '1px solid var(--color-border-light)',
                      paddingLeft: 26,
                      flexWrap: 'wrap',
                    }}
                  >
                    <Tooltip title="执行事项时自动创建 git worktree，保持工作区干净">
                      <span style={{ display: 'inline-flex', alignItems: 'center', gap: 6, cursor: 'pointer' }}>
                        <Switch
                          size="small"
                          checked={!!dir.git_worktree_enabled}
                          onChange={(v) => handleToggleWorktree(dir.id, 'gitWorktreeEnabled', v)}
                        />
                        <span style={{ fontSize: 12, color: 'var(--color-text-secondary)' }}>Git Worktree</span>
                        <QuestionCircleOutlined style={{ color: 'var(--color-text-tertiary)', fontSize: 12 }} />
                      </span>
                    </Tooltip>
                    <Tooltip title="执行结束后自动清理 worktree 目录（需先开启 Worktree）">
                      <span style={{ display: 'inline-flex', alignItems: 'center', gap: 6, cursor: 'pointer' }}>
                        <Switch
                          size="small"
                          checked={!!dir.auto_cleanup}
                          disabled={!dir.git_worktree_enabled}
                          onChange={(v) => handleToggleWorktree(dir.id, 'autoCleanup', v)}
                        />
                        <span style={{
                          fontSize: 12,
                          color: !dir.git_worktree_enabled ? 'var(--color-text-tertiary)' : 'var(--color-text-secondary)',
                        }}>
                          自动清理
                        </span>
                        <QuestionCircleOutlined style={{ color: 'var(--color-text-tertiary)', fontSize: 12 }} />
                      </span>
                    </Tooltip>
                  </div>
                </div>
              ))}
            </div>
          )}
        </Spin>
      </div>
    </PageCard>
  );
}
