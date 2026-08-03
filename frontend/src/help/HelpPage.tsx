// 帮助页面：独立全屏窗口，左菜单 + 右内容（PageCard 包裹）。
//
// 设计要点（基于 UI/UX 设计系统 Minimalism 风格）：
// 1. 取代旧 HelpDrawer（窄抽屉），改为占满主内容区的「新窗口」形态。
// 2. 左侧菜单分 4 组（概览/工作/观察/配置），每组带标题；
//    菜单项 hover 主色浅底、选中态左侧主色高亮条 + 浅蓝底。
// 3. 右侧 PageCard：header 主色图标徽章 + 标题，右上角关闭按钮；
//    内容区行高 1.75，标题字号梯度。
// 4. 路由：URL 形如 #/help/<pageId> 或 #/help/<pageId>/<featureId>，
//    HelpPage 监听 popstate 同步 selectedKey；
//    点击菜单项 pushState 更新 URL，可直达、可分享、刷新可恢复。
// 5. 树形数据从 HELP_PAGES 派生，节点 key 编码：
//    页面='p:<pageId>', 功能点='f:<pageId>/<featureId>'。

import { useMemo, useState, useEffect, useCallback, useRef } from 'react';
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

/**
 * 把选中节点 key 转成帮助路由 URL。
 *
 * 规则：
 * - 页面节点 'p:<pageId>' → #/help/<pageId>
 * - 功能点节点 'f:<pageId>/<featureId>' → #/help/<pageId>/<featureId>
 * - 其他 → #/help
 *
 * @param selectedKey 树节点 key
 * @returns 帮助路由 hash URL
 */
function keyToHelpUrl(selectedKey: string): string {
  // 页面节点：'p:<pageId>' → #/help/<pageId>
  if (selectedKey.startsWith('p:')) {
    const pageId = selectedKey.slice(2);
    return `#/help/${pageId}`;
  }
  // 功能点节点：'f:<pageId>/<featureId>' → #/help/<pageId>/<featureId>
  if (selectedKey.startsWith('f:')) {
    const rest = selectedKey.slice(2);
    const slashIdx = rest.indexOf('/');
    if (slashIdx < 0) return '#/help';
    const pageId = rest.slice(0, slashIdx);
    const featureId = rest.slice(slashIdx + 1);
    return `#/help/${pageId}/${featureId}`;
  }
  return '#/help';
}

/**
 * 从当前 URL hash 解析出帮助选中节点 key。
 *
 * 解析规则（与 keyToHelpUrl 互逆）：
 * - #/help 或 #/help/ 或无 help 段 → ''（空串表示未命中帮助路由）
 * - #/help/<pageId> → 'p:<pageId>'
 * - #/help/<pageId>/<featureId> → 'f:<pageId>/<featureId>'
 *
 * @returns 选中节点 key，未命中帮助路由返回 ''
 */
function helpKeyFromHash(): string {
  const hash = window.location.hash || '';
  // 去掉前导 #，切出 path 段
  const hashWithoutHash = hash.startsWith('#') ? hash.slice(1) : hash;
  const [path] = hashWithoutHash.split('?', 2);
  const segments = (path || '').split('/').filter(Boolean);
  // 第一段必须是 'help'
  if (segments[0] !== 'help') return '';
  // #/help 无后续段 → 默认帮助首页
  if (segments.length < 2 || !segments[1]) return 'p:_overview';
  const pageId = segments[1];
  // 有第三段则是功能点
  if (segments.length >= 3 && segments[2]) {
    return `f:${pageId}/${segments[2]}`;
  }
  return `p:${pageId}`;
}

/**
 * 判断当前 URL 是否为帮助路由。
 *
 * @returns 是帮助路由返回 true
 */
function isHelpRoute(): boolean {
  const hash = window.location.hash || '';
  const hashWithoutHash = hash.startsWith('#') ? hash.slice(1) : hash;
  const [path] = hashWithoutHash.split('?', 2);
  const segments = (path || '').split('/').filter(Boolean);
  return segments[0] === 'help';
}

/** 帮助页面容器：左菜单 + 右 PageCard 内容。 */
export function HelpPage({ open, onClose, activeView, hasDetail, isMobile }: HelpPageProps) {
  // 默认选中当前页面总览
  const defaultPageId = useDefaultPageId(activeView, hasDetail);
  const defaultSelectedKey = `p:${defaultPageId}`;
  const [selectedKey, setSelectedKey] = useState(defaultSelectedKey);
  // 展开的节点：默认展开当前页面所属分组下的当前页面
  const [expandedKeys, setExpandedKeys] = useState<string[]>([defaultSelectedKey]);
  // 标志：是否由 URL 驱动的选中，避免 handleSelect 回写 URL 时循环
  const fromUrlRef = useRef(false);

  // open 变化时（帮助被打开），若 URL 不是帮助路由，则 pushState 到帮助首页
  useEffect(() => {
    if (!open) return;
    if (!isHelpRoute()) {
      // 首次打开帮助：URL 跳到 #/help，用当前视图对应的 pageId
      const initialKey = `p:${defaultPageId}`;
      const helpUrl = keyToHelpUrl(initialKey);
      window.history.pushState(null, '', helpUrl);
      setSelectedKey(initialKey);
      setExpandedKeys([initialKey]);
    } else {
      // 刷新或直达：从 URL 恢复 selectedKey
      const key = helpKeyFromHash();
      if (key) {
        setSelectedKey(key);
        setExpandedKeys([key]);
      }
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);

  // 监听 popstate（浏览器后退/前进）与外部 pushState 改 URL
  useEffect(() => {
    function onPopState() {
      if (!isHelpRoute()) {
        // URL 已离开帮助路由，触发关闭
        onClose();
        return;
      }
      const key = helpKeyFromHash();
      if (key) {
        fromUrlRef.current = true;
        setSelectedKey(key);
        setExpandedKeys([key]);
      }
    }
    window.addEventListener('popstate', onPopState);
    return () => window.removeEventListener('popstate', onPopState);
  }, [onClose]);

  // 视图切换时（帮助未打开），重新选中当前页面总览并展开
  useEffect(() => {
    if (open) return; // 打开态下不改，由 URL 驱动
    const newKey = `p:${defaultPageId}`;
    setSelectedKey(newKey);
    setExpandedKeys([newKey]);
  }, [defaultPageId, open]);

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
    // 同步 URL：pushState 到对应的帮助路由
    const helpUrl = keyToHelpUrl(nextKey);
    window.history.pushState(null, '', helpUrl);
  }, [expandedKeys]);

  // 关闭帮助：history.back 回原视图，由 popstate 监听器同步 helpDrawerOpen
  const handleClose = useCallback(() => {
    // 若当前已在帮助路由，回退一步；否则直接触发 onClose
    if (isHelpRoute()) {
      window.history.back();
    } else {
      onClose();
    }
  }, [onClose]);

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
      <main className="ntd-help-main" style={{ flex: 1, minWidth: 0, display: 'flex', flexDirection: 'column', padding: isMobile ? 0 : 16 }}>
        <PageCard
          icon={<span className="ntd-help-card-icon">📖</span>}
          title={pageTitle}
          extra={
            <Button
              type="text"
              size="small"
              icon={<CloseOutlined />}
              onClick={handleClose}
              aria-label="关闭帮助"
            />
          }
          contentStyle={{ overflow: 'auto' }}
          style={{ flex: 1, minHeight: 0 }}
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
  );
}
