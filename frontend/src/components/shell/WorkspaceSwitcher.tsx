import { useEffect, useMemo, useState, useCallback } from 'react';
import { Button, Dropdown, Modal, Form, Input, App } from 'antd';
import type { MenuProps } from 'antd';
import { FolderOutlined, FolderOpenOutlined, SettingOutlined, PlusOutlined, DownOutlined } from '@ant-design/icons';
import * as db from '@/utils/database';
import type { ProjectDirectory } from '@/types';

export type WorkspaceSwitcherMode = 'full' | 'compact';

interface QuickAddFormValues {
  name: string;
  path: string;
}

interface WorkspaceSwitcherProps {
  /** 当前选中的工作空间 ID（project_directories.id），唯一键 */
  value: number | null;
  /** 选中工作空间后回传 id */
  onChange: (workspaceId: number | null) => void;
  /** 管理工作空间回调（可选，不提供时隐藏该菜单项） */
  onManage?: () => void;
  /** 是否显示下拉菜单中"新建工作空间"选项，默认 false */
  showAddOption?: boolean;
  mode?: WorkspaceSwitcherMode;
}

/**
 * 工作空间切换器：全局唯一的空间选择控件。
 *
 * 交互：
 * - full 模式：左侧导航顶部下拉按钮，显示当前选中空间名 + 下拉箭头
 * - compact 模式：图标按钮，仅用于极窄场景
 *
 * 下拉菜单支持：
 * - 从已有空间列表选择
 * - "新建工作空间"选项（showAddOption=true 时显示），点击弹出内置 Modal
 * - "管理工作空间"选项（onManage 提供时显示）
 */
export function WorkspaceSwitcher({
  value,
  onChange,
  onManage,
  showAddOption = false,
  mode = 'full',
}: WorkspaceSwitcherProps) {
  const { message } = App.useApp();
  const [dirs, setDirs] = useState<ProjectDirectory[]>([]);
  const [loading, setLoading] = useState(false);
  const [quickAddOpen, setQuickAddOpen] = useState(false);
  const [quickAddSaving, setQuickAddSaving] = useState(false);
  const [quickAddForm] = Form.useForm<QuickAddFormValues>();

  const loadDirs = useCallback(async () => {
    setLoading(true);
    try {
      const list = await db.getProjectDirectories();
      setDirs(list);
    } catch (err) {
      message.error(`加载工作空间失败: ${err instanceof Error ? err.message : '未知错误'}`);
      setDirs([]);
    } finally {
      setLoading(false);
    }
  }, [message]);

  useEffect(() => { loadDirs(); }, [loadDirs]);

  useEffect(() => {
    const handler = () => loadDirs();
    window.addEventListener('projectDirectoryAdded', handler);
    return () => window.removeEventListener('projectDirectoryAdded', handler);
  }, [loadDirs]);

  const selectedLabel = useMemo(() => {
    if (value == null) return '请选择工作空间';
    const found = dirs.find(d => d.id === value);
    return found?.name || String(value);
  }, [dirs, value]);

  const menuItems = useMemo<NonNullable<MenuProps['items']>>(() => {
    const items: NonNullable<MenuProps['items']> = [
      ...dirs.map(dir => ({
        key: String(dir.id),
        label: dir.name,
        icon: <FolderOpenOutlined />,
      })),
    ];
    if (showAddOption) {
      items.push(
        { type: 'divider' as const },
        {
          key: '__add__',
          label: '新建工作空间',
          icon: <PlusOutlined />,
        },
      );
    }
    if (onManage) {
      items.push(
        { type: 'divider' as const },
        {
          key: '__manage__',
          label: '管理工作空间',
          icon: <SettingOutlined />,
        },
      );
    }
    return items;
  }, [dirs, showAddOption, onManage]);

  const onMenuClick = useCallback<NonNullable<MenuProps['onClick']>>(({ key }) => {
    if (key === '__manage__') {
      onManage?.();
      return;
    }
    if (key === '__add__') {
      setQuickAddOpen(true);
      return;
    }
    const id = Number(key);
    if (Number.isFinite(id)) onChange(id);
  }, [onChange, onManage]);

  const handleQuickAdd = useCallback(async () => {
    const values = await quickAddForm.validateFields();
    setQuickAddSaving(true);
    try {
      const created = await db.createProjectDirectory(values.path.trim(), values.name.trim());
      message.success('工作空间已创建');
      quickAddForm.resetFields();
      setQuickAddOpen(false);
      const dirs = await db.getProjectDirectories();
      setDirs(dirs);
      onChange(created.id);
      window.dispatchEvent(new CustomEvent('projectDirectoryAdded', { detail: { id: created.id } }));
    } catch (e) {
      message.error(`创建失败：${(e as Error).message}`);
    } finally {
      setQuickAddSaving(false);
    }
  }, [quickAddForm, message, onChange]);

  // 新建工作空间 Modal：compact 与 full 模式共用
  const quickAddModal = (
    <Modal
      title="新建工作空间"
      open={quickAddOpen}
      onCancel={() => {
        if (quickAddSaving) return;
        quickAddForm.resetFields();
        setQuickAddOpen(false);
      }}
      onOk={handleQuickAdd}
      confirmLoading={quickAddSaving}
      okText="保存并使用"
      cancelText="取消"
      destroyOnClose
      maskClosable={!quickAddSaving}
    >
      <Form form={quickAddForm} layout="vertical" preserve={false}>
        <Form.Item label="工作空间名称" name="name" rules={[{ required: true, message: '请输入工作空间名称' }]}>
          <Input placeholder="例如：ntd 官网" autoFocus />
        </Form.Item>
        <Form.Item label="目录路径" name="path" rules={[{ required: true, message: '请输入目录路径' }]}>
          <Input placeholder="例如：/Users/me/projects/ntd-site" />
        </Form.Item>
      </Form>
    </Modal>
  );

  if (mode === 'compact') {
    return (
      <>
        <Dropdown menu={{ items: menuItems, onClick: onMenuClick }} trigger={['click']} placement="bottomLeft">
          <Button
            type="text"
            className="ntd-workspace-switcher-compact"
            icon={<FolderOutlined />}
            loading={loading}
            aria-label="切换工作空间"
            data-testid="left-rail-workspace"
          />
        </Dropdown>
        {quickAddModal}
      </>
    );
  }

  return (
    <>
      <Dropdown menu={{ items: menuItems, onClick: onMenuClick }} trigger={['click']} placement="bottomLeft">
        <Button
          type="text"
          className="ntd-workspace-switcher"
          loading={loading}
          aria-label="切换工作空间"
          data-testid="left-rail-workspace-switcher"
        >
          <span className="ntd-workspace-switcher-left">
            <FolderOutlined style={{ color: 'var(--color-primary)' }} />
            <span className="ntd-workspace-switcher-label" title={selectedLabel}>{selectedLabel}</span>
          </span>
          <span className="ntd-workspace-switcher-right">
            <DownOutlined style={{ fontSize: 10, color: 'var(--color-text-tertiary)' }} />
          </span>
        </Button>
      </Dropdown>
      {quickAddModal}
    </>
  );
}
