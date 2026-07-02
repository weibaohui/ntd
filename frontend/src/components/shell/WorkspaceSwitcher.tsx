import { useEffect, useMemo, useState, useCallback } from 'react';
import { Button, Dropdown, App } from 'antd';
import type { MenuProps } from 'antd';
import { FolderOutlined, FolderOpenOutlined, SettingOutlined, DownOutlined } from '@ant-design/icons';
import * as db from '@/utils/database';
import type { ProjectDirectory } from '@/types';

export type WorkspaceSwitcherMode = 'full' | 'compact';

interface WorkspaceSwitcherProps {
  /** 当前选中的工作空间 ID（project_directories.id），唯一键 */
  value: number | null;
  /** 选中工作空间后回传 id */
  onChange: (workspaceId: number) => void;
  onManage: () => void;
  mode?: WorkspaceSwitcherMode;
}

/**
 * 工作空间切换器（旧版交互：Dropdown 列表 + “管理工作空间”）。
 * 目标：保留你原来觉得“顺手”的选择方式，同时把入口搬到左侧导航顶部。
 */
export function WorkspaceSwitcher({ value, onChange, onManage, mode = 'full' }: WorkspaceSwitcherProps) {
  const { message } = App.useApp();
  const [dirs, setDirs] = useState<ProjectDirectory[]>([]);
  const [loading, setLoading] = useState(false);

  /**
   * 拉取工作空间目录列表。
   * 失败时不阻塞页面，只给出轻提示，避免影响用户继续操作。
   */
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

  useEffect(() => {
    loadDirs();
  }, [loadDirs]);

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
      { type: 'divider' as const },
      {
        key: '__manage__',
        label: '管理工作空间',
        icon: <SettingOutlined />,
      },
    ];
    return items;
  }, [dirs]);

  const onMenuClick = useCallback<NonNullable<MenuProps['onClick']>>(({ key }) => {
    if (key === '__manage__') {
      onManage();
      return;
    }
    // menu key 是 dir.id 字符串化；解析回 number 抛出给上层
    const id = Number(key);
    if (Number.isFinite(id)) onChange(id);
  }, [onChange, onManage]);

  if (mode === 'compact') {
    return (
      <Dropdown
        menu={{ items: menuItems, onClick: onMenuClick }}
        trigger={['click']}
        placement="bottomLeft"
      >
        <Button
          type="text"
          className="ntd-workspace-switcher-compact"
          icon={<FolderOutlined />}
          loading={loading}
          aria-label="切换工作空间"
          data-testid="left-rail-workspace"
        />
      </Dropdown>
    );
  }

  return (
    <Dropdown
      menu={{ items: menuItems, onClick: onMenuClick }}
      trigger={['click']}
      placement="bottomLeft"
    >
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
  );
}
