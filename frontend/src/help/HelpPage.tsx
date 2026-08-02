// 帮助页面：独立全屏窗口，左菜单 + 右内容（PageCard 包裹）。
//
// 设计要点（基于 UI/UX 设计系统 Minimalism 风格）：
// 1. 取代旧 HelpDrawer（窄抽屉），改为占满主内容区的「新窗口」形态。
// 2. 左侧菜单分 4 组（概览/工作/观察/配置），每组带标题；
//    菜单项 hover 主色浅底、选中态左侧主色高亮条 + 浅蓝底。
// 3. 右侧 PageCard：header 主色图标徽章 + 标题，右上角关闭按钮；
//    内容区限宽 760px 居中，行高 1.75，标题字号梯度。
// 4. 树形数据从 HELP_PAGES 派生，节点 key 编码：
//    页面='p:<pageId>', 功能点='f:<pageId>/<featureId>'。

import { useMemo, useState, useEffect } from 'react';
import { Empty, Button } from 'antd';
import { CloseOutlined } from '@ant-design/icons';
import { findHelpPage, loadHelpDoc, useDefaultPageId } from './useHelpContent';
import { decideTreeSelect } from './helpTreeSelect';
import { HelpContentRenderer } from './HelpContentRenderer';
import { PageCard } from '@/components/common/PageCard';
import type { View } from '@/hooks/useViewState';

interface HelpPageProps {
  /** 是否展示帮助页面。 */
  open: boolean;
  /** 关闭帮助页面的回调。 */
  onClose: () => void;
  /** 当前所在视图，用于树形默认选中。 */
  activeView: View;
  /** 是否处于详情形态（todoDetailId/loopDetailId/taskDetailId != null）。 */
  hasDetail: boolean;
  /** 是否移动端：移动端菜单与内容上下排列。 */
  isMobile?: boolean;
}

/** 菜单分组定义：每组对应 HELP_PAGES 的一段连续区间。 */
interface MenuGroup {
  /** 分组标题。 */
  title: string;
  /** 该组包含的页面 pageId 列表。 */
  pageIds: string[];
}

/**
 * 帮助菜单分组配置。
 *
 * 与 LeftRail 的分组一致：概览 / 工作 / 观察 / 配置。
 * 顺序即展示顺序。
 */
const MENU_GROUPS: MenuGroup[] = [
  { title: '概览', pageIds: ['_overview', 'dashboard', 'onboarding'] },
  { title: '工作', pageIds: ['tasks-list', 'tasks-detail', 'todos-list', 'todos-detail', 'loops-list', 'loops-detail', 'processes'] },
  { title: '观察', pageIds: ['messages', 'blackboard', 'memorial'] },
  { title: '配置', pageIds: ['settings-skills', 'settings-experts', 'settings-executors', 'settings-bots', 'settings-pd', 'settings-more'] },
];

/**
 * 根据选中节点 key 解析出要渲染的 md 文件名。
 *
 * @param selectedKey 树节点 key
 * @returns md 文件名，找不到返回 ''
 */
function resolveDocFile(selectedKey: string): string {
  // 页面节点：'p:<pageId>'
  if (selectedKey.startsWith('p:')) {
    const page = findHelpPage(selectedKey.slice(2));
    return page ? page.overviewDoc : '';
  }
  // 功能点节点：'f:<pageId>/<featureId>'
  if (selectedKey.startsWith('f:')) {
    const rest = selectedKey.slice(2);
    const slashIdx = rest.indexOf('/');
    if (slashIdx < 0) return '';
    const page = findHelpPage(rest.slice(0, slashIdx));
    if (!page) return '';
    const feature = page.features.find(f => f.id === rest.slice(slashIdx + 1));
    return feature ? feature.docFile : '';
  }
  return '';
}

/**
 * 根据选中节点 key 解析出展示标题。
 *
 * @param selectedKey 树节点 key
 * @returns 标题文本
 */
function resolveTitle(selectedKey: string): string {
  if (selectedKey.startsWith('p:')) {
    const page = findHelpPage(selectedKey.slice(2));
    return page ? page.title : '帮助';
  }
  if (selectedKey.startsWith('f:')) {
    const rest = selectedKey.slice(2);
    const slashIdx = rest.indexOf('/');
    if (slashIdx < 0) return '帮助';
    const page = findHelpPage(rest.slice(0, slashIdx));
    const feature = page?.features.find(f => f.id === rest.slice(slashIdx + 1));
    return feature ? feature.title : '帮助';
  }
  return '帮助';
}

/** 帮助页面容器：左菜单 + 右 PageCard 内容。 */
export function HelpPage({ open, onClose, activeView, hasDetail, isMobile }: HelpPageProps) {
  // 默认选中当前页面总览
  const defaultPageId = useDefaultPageId(activeView, hasDetail);
  const defaultSelectedKey = `p:${defaultPageId}`;
  const [selectedKey, setSelectedKey] = useState(defaultSelectedKey);
  // 展开的节点：默认展开当前页面所属分组下的当前页面
  const [expandedKeys, setExpandedKeys] = useState<string[]>([defaultSelectedKey]);

  // 视图切换时，重新选中当前页面总览并展开
  useEffect(() => {
    const newKey = `p:${defaultPageId}`;
    setSelectedKey(newKey);
    setExpandedKeys([newKey]);
  }, [defaultPageId]);

  // 解析当前选中节点对应的 md 文件名与标题
  const docFile = useMemo(() => resolveDocFile(selectedKey), [selectedKey]);
  const docSource = useMemo(() => loadHelpDoc(docFile), [docFile]);
  const pageTitle = useMemo(() => resolveTitle(selectedKey), [selectedKey]);

  // 树节点选中回调。
  // 决策逻辑抽在 helpTreeSelect.decideTreeSelect（纯函数，便于单测）。
  function handleSelect(key: string) {
    // 用 antd Tree 的单选语义模拟：构造单元素数组
    const { selectedKey: nextKey, expandKey } = decideTreeSelect([key], expandedKeys);
    if (nextKey === null) return;
    setSelectedKey(nextKey);
    if (expandKey !== null) {
      setExpandedKeys(current => (current.includes(expandKey) ? current : [...current, expandKey]));
    }
  }

  // open=false 时不渲染，由父组件控制挂载
  if (!open) return null;

  return (
    <div
      className="ntd-help-page"
      style={{
        position: 'absolute',
        inset: 0,
        zIndex: 100,
        display: 'flex',
        flexDirection: isMobile ? 'column' : 'row',
        background: 'var(--color-bg-base, #f8fafc)',
      }}
    >
      {/* 左侧菜单 */}
      <aside
        className="ntd-help-sidebar"
        style={{
          width: isMobile ? '100%' : 248,
          flexShrink: 0,
          maxHeight: isMobile ? 260 : undefined,
          overflow: 'auto',
          borderRight: isMobile ? undefined : '1px solid var(--color-border-light, #e2e8f0)',
          borderBottom: isMobile ? '1px solid var(--color-border-light, #e2e8f0)' : undefined,
          background: 'var(--color-bg-elevated, #fff)',
        }}
      >
        {/* 菜单头部标题 */}
        <div className="ntd-help-sidebar-header">
          <span className="ntd-help-sidebar-title">帮助文档</span>
        </div>

        {/* 分组菜单 */}
        <nav className="ntd-help-menu">
          {MENU_GROUPS.map(group => (
            <div key={group.title} className="ntd-help-menu-group">
              <div className="ntd-help-menu-group-title">{group.title}</div>
              {group.pageIds.map(pageId => {
                const page = findHelpPage(pageId);
                if (!page) return null;
                const pageKey = `p:${pageId}`;
                const isPageSelected = selectedKey === pageKey;
                const isPageExpanded = expandedKeys.includes(pageKey);
                return (
                  <div key={pageId} className="ntd-help-menu-item-wrap">
                    <button
                      type="button"
                      className={`ntd-help-menu-item ${isPageSelected ? 'is-selected' : ''}`}
                      onClick={() => handleSelect(pageKey)}
                    >
                      <span className="ntd-help-menu-item-label">{page.title}</span>
                    </button>
                    {/* 功能点子菜单：展开时显示 */}
                    {isPageExpanded && page.features.length > 0 && (
                      <div className="ntd-help-menu-sub">
                        {page.features.map(feature => {
                          const featureKey = `f:${pageId}/${feature.id}`;
                          const isFeatureSelected = selectedKey === featureKey;
                          return (
                            <button
                              key={feature.id}
                              type="button"
                              className={`ntd-help-menu-sub-item ${isFeatureSelected ? 'is-selected' : ''}`}
                              onClick={() => handleSelect(featureKey)}
                            >
                              {feature.title}
                            </button>
                          );
                        })}
                      </div>
                    )}
                  </div>
                );
              })}
            </div>
          ))}
        </nav>
      </aside>

      {/* 右侧内容：PageCard 包裹 */}
      <main className="ntd-help-main" style={{ flex: 1, minWidth: 0, display: 'flex', flexDirection: 'column', padding: isMobile ? 0 : 16 }}>
        <PageCard
          icon={<span className="ntd-help-card-icon">📖</span>}
          title={pageTitle}
          extra={
            <Button
              type="text"
              size="small"
              icon={<CloseOutlined />}
              onClick={onClose}
              aria-label="关闭帮助"
            />
          }
          contentStyle={{ overflow: 'auto' }}
          style={{ flex: 1, minHeight: 0 }}
        >
          {/* 内容区限宽居中，保证可读性（line-length 65-75 字符） */}
          <div className="ntd-help-content">
            {docSource ? (
              <HelpContentRenderer source={docSource} />
            ) : (
              <Empty description="暂无帮助内容" />
            )}
          </div>
        </PageCard>
      </main>
    </div>
  );
}
