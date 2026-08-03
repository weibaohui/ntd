// 帮助页面：独立页面（window.open 打开的新窗口），左菜单 + 右内容（PageCard 包裹）。
//
// 设计要点（基于 UI/UX 设计系统 Minimalism 风格）：
// 1. 以独立浏览器页面打开（target=_blank 语义），不再是主应用内的覆盖层。
// 2. URL #/help/<pageId>/<featureId> 驱动选中态；刷新可恢复、可分享、可直达。
// 3. 左侧菜单分 4 组（概览/工作/观察/配置），每组带标题；
//    菜单项 hover 主色浅底、选中态左侧主色高亮条 + 浅蓝底。
// 4. 右侧 PageCard：header 主色图标徽章 + 标题；
//    内容区行高 1.75，标题字号梯度。无关闭按钮（正常浏览器页面）。
// 5. 树形数据从 HELP_PAGES 派生，节点 key 编码：
//    页面='p:<pageId>', 功能点='f:<pageId>/<featureId>'。

import { useMemo, useState, useEffect, useCallback, useRef } from 'react';
import { Empty } from 'antd';
import { findHelpPage, loadHelpDoc } from './useHelpContent';
import { decideTreeSelect } from './helpTreeSelect';
import { HelpContentRenderer } from './HelpContentRenderer';
import { PageCard } from '@/components/common/PageCard';
import { useTheme } from '@/hooks/useTheme';

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
 * - #/help 或 #/help/ 或无 help 段 → 'p:_overview'（帮助首页）
 * - #/help/<pageId> → 'p:<pageId>'
 * - #/help/<pageId>/<featureId> → 'f:<pageId>/<featureId>'
 *
 * @returns 选中节点 key，未命中帮助路由返回 'p:_overview'
 */
function helpKeyFromHash(): string {
  const hash = window.location.hash || '';
  // 去掉前导 #，切出 path 段
  const hashWithoutHash = hash.startsWith('#') ? hash.slice(1) : hash;
  const [path] = hashWithoutHash.split('?', 2);
  const segments = (path || '').split('/').filter(Boolean);
  // 第一段必须是 'help'
  if (segments[0] !== 'help') return 'p:_overview';
  // #/help 无后续段 → 默认帮助首页
  if (segments.length < 2 || !segments[1]) return 'p:_overview';
  const pageId = segments[1];
  // 有第三段则是功能点
  if (segments.length >= 3 && segments[2]) {
    return `f:${pageId}/${segments[2]}`;
  }
  return `p:${pageId}`;
}

/** 帮助页面容器：独立页面，左菜单 + 右 PageCard 内容。 */
export function HelpPage() {
  // 主题：帮助独立页面需要自己的主题上下文（主应用不在同页面）
  const { themeMode } = useTheme();
  // 从 URL 恢复初始选中态
  const initialKey = helpKeyFromHash();
  const [selectedKey, setSelectedKey] = useState(initialKey);
  // 展开的节点：默认展开当前选中节点所属分组下的当前页面
  const [expandedKeys, setExpandedKeys] = useState<string[]>([initialKey]);

  // 标志：是否由 URL 驱动的选中，避免 handleSelect 回写 URL 时循环
  const fromUrlRef = useRef(false);

  // 监听 popstate（浏览器后退/前进）与 hashchange 同步 selectedKey
  useEffect(() => {
    function onNavChange() {
      const key = helpKeyFromHash();
      if (key) {
        fromUrlRef.current = true;
        setSelectedKey(key);
        setExpandedKeys([key]);
      }
    }
    window.addEventListener('popstate', onNavChange);
    window.addEventListener('hashchange', onNavChange);
    return () => {
      window.removeEventListener('popstate', onNavChange);
      window.removeEventListener('hashchange', onNavChange);
    };
  }, []);

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

  // 判断移动端：窗口宽度小于 768px 时菜单与内容上下排列
  const isMobile = window.innerWidth < 768;

  return (
    <div
      className="ntd-help-page"
      data-theme={themeMode}
      style={{
        width: '100vw',
        height: '100vh',
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
