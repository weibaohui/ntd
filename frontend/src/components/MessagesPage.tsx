import { useState, useEffect } from 'react';
import { Spin, Empty } from 'antd';
import { MessageOutlined } from '@ant-design/icons';
import { PageCard } from '@/components/common/PageCard';
import { WorkspaceMessageConfigPage } from '@/components/settings/workspace/WorkspaceMessageConfigPage';
import * as db from '@/utils/database';
import type { ProjectDirectory } from '@/utils/database';

/**
 * 消息页面 props。
 * workspaceId：来自左上角 WorkspaceSwitcher 的当前选中工作空间 ID，null 表示未选。
 * onManageWorkspace：未选中工作空间时，引导用户前往工作空间管理页选用其一。
 */
interface MessagesPageProps {
  workspaceId: number | null;
  onManageWorkspace: () => void;
}

/**
 * 消息页面（独立菜单项）。
 *
 * 设计意图：把原本嵌入在工作空间管理页内的「消息配置页」提升为左侧菜单的独立页面，
 * 用户点开后直接按左上角所选 workspace id 联动渲染，无需先进入工作空间再下钻。
 *
 * 渲染策略：
 * - 未选中工作空间：给出空态提示，引导前往工作空间管理。
 * - 已选中：按 id 拉取 ProjectDirectory（页面标题需要 workspace.name），交给原
 *   WorkspaceMessageConfigPage 渲染，保持与原嵌入版完全一致的 UI 与交互。
 *   onBack 实现为回到工作空间管理页，与原嵌入入口的返回语义保持一致。
 */
export function MessagesPage({ workspaceId, onManageWorkspace }: MessagesPageProps) {
  // 当前选中的工作空间对象；仅 workspaceId 有效时加载，避免无谓请求。
  const [workspace, setWorkspace] = useState<ProjectDirectory | null>(null);
  // 加载态：用于区分「正在拉取」与「拉取完但未选中」两种空态，给用户不同提示。
  const [loading, setLoading] = useState(false);

  /**
   * 段落总览：workspaceId 变化时重新拉取对应的 ProjectDirectory。
   * 用 db.getProjectDirectories 取全量再按 id 过滤——后端没有单查接口，
   * 且工作空间数量通常个位数，全量取再本地查找比逐个加接口更经济。
   * 拉取失败时清空 workspace，让页面落到「未选中」空态分支，避免殁留旧数据。
   */
  useEffect(() => {
    if (workspaceId == null) {
      // 未选中工作空间：直接清空，不发请求。
      setWorkspace(null);
      return;
    }
    setLoading(true);
    db.getProjectDirectories()
      .then((dirs) => {
        // 按 id 在全量列表里找当前选中的；找不到（刚被删除等）也落到空态。
        const matched = dirs.find((d) => d.id === workspaceId) ?? null;
        setWorkspace(matched);
      })
      .catch(() => setWorkspace(null))
      .finally(() => setLoading(false));
  }, [workspaceId]);

  // 分支 1：正在加载工作空间信息，显示骨架屏避免空态闪烁。
  if (loading) {
    return (
      <PageCard icon={<MessageOutlined />} title="消息">
        <div style={{ display: 'flex', justifyContent: 'center', padding: 48 }}>
          <Spin />
        </div>
      </PageCard>
    );
  }

  // 分支 2：未选中工作空间（或加载后仍无对应项），引导用户先选用工作空间。
  if (workspace == null) {
    return (
      <PageCard icon={<MessageOutlined />} title="消息">
        <Empty
          description="请先在左上角选择一个工作空间，或前往工作空间管理新建"
          style={{ padding: 48 }}
        >
          {/* 引导按钮：跳到工作空间管理页，与原嵌入入口的「返回到工作空间列表」语义对齐 */}
          <a onClick={onManageWorkspace} style={{ cursor: 'pointer' }}>
            前往工作空间管理
          </a>
        </Empty>
      </PageCard>
    );
  }

  // 分支 3：已有选中工作空间，原样复用 WorkspaceMessageConfigPage 渲染。
  // onBack 落到 onManageWorkspace：从消息页「返回」即回到工作空间管理，与原嵌入版一致。
  return (
    <WorkspaceMessageConfigPage
      workspace={workspace}
      onBack={onManageWorkspace}
    />
  );
}
