// 帮助页面：内嵌大弹窗（antd Modal），左菜单 + 右内容（PageCard 包裹）。
//
// 设计要点（基于 UI/UX 设计系统 Minimalism 风格）：
// 1. 以 antd Modal 大弹窗形态打开（width 80vw / top 5vh），不再新开浏览器窗口。
// 2. 关闭即全部关闭：内部导航的多个帮助页都在同一弹窗内，关 Modal 一并关闭。
// 3. 选中态由 React state 驱动，不走 URL 路由——弹窗关闭后历史不留 help 路由污染。
// 4. 左侧菜单分 4 组（概览/工作/观察/配置），每组带标题；
//    菜单项 hover 主色浅底、选中态左侧主色高亮条 + 浅蓝底。
// 5. 右侧 PageCard：header 主色图标徽章 + 标题，右上角关闭按钮；
//    内容区行高 1.75，标题字号梯度。
// 6. 暗色主题：Modal body 背景用 var(--color-bg-base)，消除右侧白边刺眼问题。
// 7. 树形数据从 HELP_PAGES 派生，节点 key 编码：
//    页面='p:<pageId>', 功能点='f:<pageId>/<featureId>'。

import { useMemo, useState, useCallback } from 'react';
import { Empty, Modal, Button } from 'antd';
import { ExpandOutlined, CompressOutlined } from '@ant-design/icons';
import { findHelpPage, loadHelpDoc } from './useHelpContent';
import { decideTreeSelect } from './helpTreeSelect';
import { HelpContentRenderer } from './HelpContentRenderer';
import { PageCard } from '@/components/common/PageCard';
import { useIsMobile } from '@/hooks/useIsMobile';

interface HelpPageProps {
  /** 是否展示帮助弹窗。 */
  open: boolean;
  /** 关闭帮助弹窗的回调。 */
  onClose: () => void;
  /** 初始选中的 pageId，打开时直接跳到该页；未传或找不到时回退概览首页。 */
  initialPageId?: string;
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
  { title: '观察', pageIds: ['messages', 'blackboard', 'ops'] },
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

/**
 * 校验 pageId 是否已注册在 HELP_PAGES。
 *
 * @param pageId 页面 id
 * @returns 已注册返回 true
 */
function isValidPageId(pageId: string | undefined): boolean {
  if (!pageId) return false;
  return findHelpPage(pageId) != null;
}

/** 帮助弹窗：antd Modal 大窗，内嵌左菜单 + 右 PageCard 内容。 */
export function HelpPage({ open, onClose, initialPageId }: HelpPageProps) {
  const isMobile = useIsMobile();
  // 全屏切换：true 时占满整个视窗，false 时桌面端 80vw / 5vh 顶部留白
  const [isFullscreen, setIsFullscreen] = useState(false);
  // 初始选中：传入的 initialPageId 合法则用其页面 key，否则回退概览首页
  const defaultPageKey = isValidPageId(initialPageId) ? `p:${initialPageId}` : 'p:_overview';
  const [selectedKey, setSelectedKey] = useState(defaultPageKey);
  // 展开的节点：默认展开初始选中节点
  const [expandedKeys, setExpandedKeys] = useState<string[]>([defaultPageKey]);

  // open 变化时（弹窗打开）重置选中态到 initialPageId
  // 用 useMemo 派生 defaultPageKey 已在首次渲染定型；这里在 open 由 false→true 时重置
  // 避免在同一弹窗生命周期内重复打开残留上次选中态
  const [lastOpen, setLastOpen] = useState(open);
  if (open && !lastOpen) {
    // 弹窗刚被打开：重置选中态到 initialPageId
    const key = isValidPageId(initialPageId) ? `p:${initialPageId}` : 'p:_overview';
    setSelectedKey(key);
    setExpandedKeys([key]);
    setLastOpen(true);
  } else if (!open && lastOpen) {
    setLastOpen(false);
  }

  // 解析当前选中节点对应的 md 文件名与标题
  const docFile = useMemo(() => resolveDocFile(selectedKey), [selectedKey]);
  const docSource = useMemo(() => loadHelpDoc(docFile), [docFile]);
  const pageTitle = useMemo(() => resolveTitle(selectedKey), [selectedKey]);

  // 树节点选中回调。
  // 决策逻辑抽在 helpTreeSelect.decideTreeSelect（纯函数，便于单测）。
  const handleSelect = useCallback((key: string) => {
    const { selectedKey: nextKey, expandKey } = decideTreeSelect([key], expandedKeys);
    if (nextKey === null) return;
    setSelectedKey(nextKey);
    if (expandKey !== null) {
      setExpandedKeys(current => (current.includes(expandKey) ? current : [...current, expandKey]));
    }
  }, [expandedKeys]);

  // Modal 尺寸：全屏时占满视窗；桌面端 80vw / 5vh 顶部留白；移动端全屏
  const modalWidth = isFullscreen || isMobile ? '100vw' : '80vw';
  const modalStyle: React.CSSProperties = {
    top: isFullscreen || isMobile ? 0 : '5vh',
    maxWidth: '100vw',
    paddingBottom: 0,
    transition: 'top 0.2s ease',
  };
  // Modal body 内 padding 归零，让左菜单+右内容占满；
  // 背景 header/body 统一用主题底色，消除暗色下右侧白边刺眼问题
  const modalStyles = {
    header: {
      background: 'var(--color-bg-elevated, #fff)',
      borderBottom: '1px solid var(--color-border-light, #e2e8f0)',
      padding: '12px 20px',
      margin: 0,
    } as React.CSSProperties,
    body: {
      padding: 0,
      background: 'var(--color-bg-base, #f8fafc)',
      height: isFullscreen
        ? 'calc(100vh - 55px)'
        : isMobile
          ? 'calc(100vh - 55px)'
          : 'calc(85vh - 55px)',
      overflow: 'hidden',
    } as React.CSSProperties,
  };
  // 全屏按钮：放在 Modal title 旁，切换全屏/弹窗态
  const fullscreenBtn = (
    <Button
      type="text"
      size="small"
      icon={isFullscreen ? <CompressOutlined /> : <ExpandOutlined />}
      onClick={() => setIsFullscreen(prev => !prev)}
      aria-label={isFullscreen ? '退出全屏' : '全屏'}
      style={{ position: 'absolute', right: 40, top: 12 }}
    />
  );

  return (
    <Modal
      open={open}
      onCancel={onClose}
      footer={null}
      title={(
        <>
          帮助文档
          {fullscreenBtn}
        </>
      )}
      width={modalWidth}
      style={modalStyle}
      styles={modalStyles}
      destroyOnClose
      maskClosable
      keyboard
    >
      <div
        className="ntd-help-page"
        style={{
          display: 'flex',
          flexDirection: isMobile ? 'column' : 'row',
          width: '100%',
          height: '100%',
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
          {/* 分组菜单 */}
          <nav className="ntd-help-menu">
            {MENU_GROUPS.map(group => (
              <div key={group.title} className="ntd-help-menu-group">
                <div className="ntd-help-menu-group-title">{group.title}</div>
                {group.pageIds.map(pageId => {
                  const page = findHelpPage(pageId);
                  if (!page) return null;
                  const pageKey = `p:${pageId}`;
                  const isPageSelected = selectedKey === pageKey || selectedKey.startsWith(`f:${pageId}/`);
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
        {/* padding 取 0：让 PageCard 撑满右侧，避免亮色下浅灰底色露出像暗色边框 */}
        <main className="ntd-help-main" style={{ flex: 1, minWidth: 0, display: 'flex', flexDirection: 'column', padding: 0 }}>
          <PageCard
            icon={<span className="ntd-help-card-icon">📖</span>}
            title={pageTitle}
            contentStyle={{ overflow: 'auto' }}
            style={{ flex: 1, minHeight: 0, boxShadow: 'none', borderRadius: 0 }}
          >
            {/* 内容区：撑满 PageCard 宽度，行高 1.75 保证可读性 */}
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
    </Modal>
  );
}
