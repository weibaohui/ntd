import { useState, useEffect } from 'react';
import { Button, Popconfirm, Input, Space, List, Empty, Spin, message } from 'antd';
import { PlusOutlined, FolderOutlined, EditOutlined, DeleteOutlined } from '@ant-design/icons';
import * as db from '../../utils/database';
import type { ProjectDirectory } from '../../utils/database';

export function ProjectDirectoriesPanel() {
  const [projectDirectories, setProjectDirectories] = useState<ProjectDirectory[]>([]);
  const [projectDirsLoading, setProjectDirsLoading] = useState(false);
  const [newDirPath, setNewDirPath] = useState('');
  const [newDirName, setNewDirName] = useState('');
  const [addingDir, setAddingDir] = useState(false);
  const [editingDirId, setEditingDirId] = useState<number | null>(null);
  const [editingDirName, setEditingDirName] = useState('');

  const loadProjectDirectories = () => {
    setProjectDirsLoading(true);
    db.getProjectDirectories()
      .then(setProjectDirectories)
      .catch((err: any) => message.error('加载项目目录失败: ' + (err?.message || String(err))))
      .finally(() => setProjectDirsLoading(false));
  };

  useEffect(() => {
    loadProjectDirectories();
  }, []);

  const handleAddProjectDirectory = async () => {
    const path = newDirPath.trim();
    if (!path) {
      message.error('请输入目录路径');
      return;
    }
    setAddingDir(true);
    try {
      const dir = await db.createProjectDirectory(path, newDirName.trim() || undefined);
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
    try {
      await db.updateProjectDirectory(id, editingDirName.trim() || undefined);
      setProjectDirectories(prev => prev.map(d => d.id === id ? { ...d, name: editingDirName.trim() || null } : d));
      setEditingDirId(null);
      setEditingDirName('');
      message.success('更新成功');
    } catch (err: any) {
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
            添加常用项目目录，方便在为 Todo 选择执行目录时快速点选。目录路径必填，名称选填。
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
              placeholder="名称（选填）"
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
                          placeholder="输入名称"
                          size="small"
                          style={{ width: 120 }}
                          onPressEnter={() => handleUpdateProjectDirectoryName(dir.id)}
                          autoFocus
                        />
                        <Button size="small" type="primary" onClick={() => handleUpdateProjectDirectoryName(dir.id)}>保存</Button>
                        <Button size="small" onClick={() => { setEditingDirId(null); setEditingDirName(''); }}>取消</Button>
                      </div>
                    ) : (
                      <>
                        <div style={{ fontSize: 14, fontWeight: 500, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                          {dir.name || dir.path}
                        </div>
                        {dir.name && (
                          <div style={{ fontSize: 12, color: 'var(--color-text-secondary)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                            {dir.path}
                          </div>
                        )}
                      </>
                    )}
                  </div>
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
              </List.Item>
            )}
          />
        )}
      </Spin>
    </div>
  );
}
