import { useState, useEffect } from 'react';
import { Button, Popconfirm, Input, Space, List, Empty, Spin, Switch, message, Tooltip } from 'antd';
import { PlusOutlined, FolderOutlined, EditOutlined, DeleteOutlined, QuestionCircleOutlined } from '@ant-design/icons';
import * as db from '@/utils/database';
import type { ProjectDirectory } from '@/utils/database';

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

  // 每次进入页面都重新拉取一次，确保用户在其他地方新增/删除后能立刻看到
  const loadProjectDirectories = () => {
    setProjectDirsLoading(true);
    db.getProjectDirectories()
      .then(setProjectDirectories)
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

  return (
    <div style={{ maxWidth: 700 }}>
      <Spin spinning={projectDirsLoading}>
        <div style={{ marginBottom: 12, fontWeight: 600 }}>添加项目目录</div>
        <div style={{ marginBottom: 24 }}>
          <div style={{ fontSize: 13, color: 'var(--color-text-secondary)', marginBottom: 12 }}>
            添加常用项目目录。目录路径与项目名称均为必填，Todo 侧会按项目名称来选择与展示。
          </div>
          <div style={{ display: 'flex', gap: 8, alignItems: 'flex-start' }}>
            <Input
              value={newDirPath}
              onChange={(e) => setNewDirPath(e.target.value)}
              placeholder="目录路径（必填）"
              style={{ flex: 2 }}
              onPressEnter={handleAddProjectDirectory}
            />
            <Input
              value={newDirName}
              onChange={(e) => setNewDirName(e.target.value)}
              placeholder="项目名称（必填）"
              style={{ flex: 1 }}
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

        <div style={{ marginBottom: 12, fontWeight: 600 }}>已添加的目录</div>
        {projectDirectories.length === 0 ? (
          <Empty description="暂无项目目录" image={Empty.PRESENTED_IMAGE_SIMPLE} />
        ) : (
          <List
            dataSource={projectDirectories}
            renderItem={(dir) => (
              <List.Item
                style={{
                  padding: '12px',
                  background: 'var(--color-bg)',
                  borderRadius: 6,
                  marginBottom: 8,
                  border: '1px solid var(--color-border-light)',
                  display: 'block',
                }}
              >
                <div style={{ display: 'flex', alignItems: 'center', gap: 10, flex: 1, minWidth: 0 }}>
                  <FolderOutlined style={{ fontSize: 18, color: '#1890ff', flexShrink: 0 }} />
                  <div style={{ flex: 1, minWidth: 0 }}>
                    {editingDirId === dir.id ? (
                      <div style={{ display: 'flex', gap: 8, alignItems: 'center' }}>
                        <Input
                          value={editingDirName}
                          onChange={(e) => setEditingDirName(e.target.value)}
                          placeholder="输入项目名称"
                          size="small"
                          style={{ width: 160 }}
                          onPressEnter={() => handleUpdateProjectDirectoryName(dir.id)}
                          autoFocus
                        />
                        <Button size="small" type="primary" onClick={() => handleUpdateProjectDirectoryName(dir.id)}>保存</Button>
                        <Button size="small" onClick={() => { setEditingDirId(null); setEditingDirName(''); }}>取消</Button>
                      </div>
                    ) : (
                      <>
                        {/* 名称是项目维度的"主键"，固定作为第一行显示；缺失时退化到路径但给出视觉提示 */}
                        <div style={{ fontSize: 14, fontWeight: 500, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                          {dir.name || <span style={{ color: 'var(--color-warning)' }}>{dir.path}（未命名）</span>}
                        </div>
                        <div style={{ fontSize: 12, color: 'var(--color-text-secondary)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                          {dir.path}
                        </div>
                      </>
                    )}
                  </div>
                  <Space size={4}>
                    {editingDirId !== dir.id && (
                      <Button
                        type="text"
                        icon={<EditOutlined />}
                        size="small"
                        onClick={() => { setEditingDirId(dir.id); setEditingDirName(dir.name || ''); }}
                      />
                    )}
                    <Popconfirm
                      title="删除目录"
                      description={`确定要删除 "${dir.name || dir.path}" 吗？`}
                      onConfirm={() => handleDeleteProjectDirectory(dir.id)}
                    >
                      <Button type="text" danger icon={<DeleteOutlined />} size="small" />
                    </Popconfirm>
                  </Space>
                </div>
                {/* issue #643: worktree 开关区。放在主行下方独立一行，避免和编辑/删除按钮挤在同一行
                    触发布局错位。label 紧贴 Switch 表达"操作对象 + 状态"，Tooltip 提供解释。 */}
                <div style={{ display: 'flex', gap: 24, marginTop: 10, paddingLeft: 28, alignItems: 'center', flexWrap: 'wrap' }}>
                  <Tooltip title="开启后，ntd 会在该目录下执行 Todo 时自动创建 git worktree，目录非 git 仓库时会自动 init">
                    <span style={{ display: 'inline-flex', alignItems: 'center', gap: 6 }}>
                      <Switch
                        size="small"
                        checked={!!dir.git_worktree_enabled}
                        onChange={(v) => handleToggleWorktree(dir.id, 'gitWorktreeEnabled', v)}
                      />
                      <span style={{ fontSize: 12 }}>启用 Git Worktree</span>
                      <QuestionCircleOutlined style={{ color: 'var(--color-text-secondary)', fontSize: 12 }} />
                    </span>
                  </Tooltip>
                  <Tooltip title="依赖上一项。开启后执行结束（成功/失败/取消）自动删除 worktree 目录">
                    <span style={{ display: 'inline-flex', alignItems: 'center', gap: 6 }}>
                      <Switch
                        size="small"
                        checked={!!dir.auto_cleanup}
                        disabled={!dir.git_worktree_enabled}
                        onChange={(v) => handleToggleWorktree(dir.id, 'autoCleanup', v)}
                      />
                      <span style={{ fontSize: 12, color: !dir.git_worktree_enabled ? 'var(--color-text-secondary)' : undefined }}>自动清理</span>
                      <QuestionCircleOutlined style={{ color: 'var(--color-text-secondary)', fontSize: 12 }} />
                    </span>
                  </Tooltip>
                </div>
              </List.Item>
            )}
          />
        )}
      </Spin>
    </div>
  );
}
