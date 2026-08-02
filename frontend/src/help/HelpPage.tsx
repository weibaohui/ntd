// 帮助页面：独立全屏窗口，左菜单 + 右内容（PageCard 包裹）。
//
// 设计要点：
// 1. 取代旧的 HelpDrawer（窄抽屉），改为占满主内容区的「新窗口」形态，
//    与项目其他 PageCard 页面保持一致的交互与视觉风格。
// 2. 左侧 Antd Tree 展示「页面 → 功能点」两级，自动选中当前页面总览；
//    右侧用 PageCard 包裹 HelpContentRenderer，标题展示当前选中节点名。
// 3. 树形数据从 HELP_PAGES 派生，节点 key 编码：
//    页面='p:<pageId>', 功能点='f:<pageId>/<featureId>'。
// 4. 顶部右上角放「关闭」按钮，关闭后回到原视图。

import { useMemo, useState, useEffect } from 'react';
import { Tree, Empty, Button } from 'antd';
import { CloseOutlined } from '@ant-design/icons';
import type { TreeDataNode } from 'antd';
import { HELP_PAGES } from './index';
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

/**
 * 从 HELP_PAGES 派生 Antd Tree 数据。
 *
 * 页面节点 key: 'p:<pageId>'
 * 功能点节点 key: 'f:<pageId>/<featureId>'
 *
 * @returns Antd Tree 数据
 */
function useHelpTreeData(): TreeDataNode[] {
  return useMemo(() => {
    return HELP_PAGES.map(page => ({
      key: `p:${page.pageId}`,
      title: page.title,
      children: page.features.map(feature => ({
        key: `f:${page.pageId}/${feature.id}`,
        title: feature.title,
        isLeaf: true,
      })),
    }));
  }, []);
}

/**
 * 根据选中节点 key 解析出要渲染的 md 文件名。
 *
 * @param selectedKey 树节点 key
 * @returns md 文件名，找不到返回 ''
 */
function resolveDocFile(selectedKey: string): string {
  // 页面节点：'p:<pageId>'
  if (selectedKey.startsWith('p:')) {
    const pageId = selectedKey.slice(2);
    const page = findHelpPage(pageId);
    return page ? page.overviewDoc : '';
  }
  // 功能点节点：'f:<pageId>/<featureId>'
  if (selectedKey.startsWith('f:')) {
    const rest = selectedKey.slice(2);
    const slashIdx = rest.indexOf('/');
    if (slashIdx < 0) return '';
    const pageId = rest.slice(0, slashIdx);
    const featureId = rest.slice(slashIdx + 1);
    const page = findHelpPage(pageId);
    if (!page) return '';
    const feature = page.features.find(f => f.id === featureId);
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
  // 页面节点：取页面标题
  if (selectedKey.startsWith('p:')) {
    const page = findHelpPage(selectedKey.slice(2));
    return page ? page.title : '帮助';
  }
  // 功能点节点：取所属页面 + 功能点标题
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
  const treeData = useHelpTreeData();
  // 默认选中当前页面总览
  const defaultPageId = useDefaultPageId(activeView, hasDetail);
  const defaultSelectedKey = `p:${defaultPageId}`;
  const [selectedKey, setSelectedKey] = useState(defaultSelectedKey);
  // 展开的节点：默认展开当前页面
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
  // 决策逻辑抽在 helpTreeSelect.decideTreeSelect（纯函数，便于单测）：
  // 选中页面节点（'p:' 前缀）且未展开时同步展开，实现「点标题即展开子菜单」（NTD-011）。
  // 收起动作仍交给 switcher 箭头承担，避免「点已选中节点标题」与「收起」产生冲突。
  function handleSelect(keys: React.Key[]) {
    const { selectedKey: nextKey, expandKey } = decideTreeSelect(keys, expandedKeys);
    // null 表示反选手势（点已选中节点），帮助树要求始终有选中节点，忽略
    if (nextKey === null) return;
    setSelectedKey(nextKey);
    if (expandKey !== null) {
      // 函数式更新：基于 setter 回调中的最新数组去重后追加，
      // 避免与 onExpand 同批触发时读到旧闭包、覆盖较新的展开状态（PR #978 评审）
      setExpandedKeys(current => (current.includes(expandKey) ? current : [...current, expandKey]));
    }
  }

  // 树节点展开回调
  function handleExpand(keys: React.Key[]) {
    setExpandedKeys(keys.map(String));
  }

  // open=false 时不渲染，由父组件控制挂载
  if (!open) return null;

  // 左菜单宽度：桌面 220px，移动端全宽
  const menuWidth = isMobile ? '100%' : 220;

  return (
    <div
      style={{
        // 占满父容器（App Content 区域），作为「新窗口」覆盖原视图
        position: 'absolute',
        inset: 0,
        background: 'var(--color-bg-base, #fff)',
        zIndex: 100,
        display: 'flex',
        flexDirection: isMobile ? 'column' : 'row',
        gap: isMobile ? 0 : 12,
        padding: isMobile ? 0 : 12,
      }}
    >
      {/* 左侧菜单（移动端在上） */}
      <div
        style={{
          width: menuWidth,
          flexShrink: 0,
          maxHeight: isMobile ? 240 : undefined,
          overflow: 'auto',
          background: 'var(--color-bg-elevated, #fff)',
          borderRadius: 'var(--radius-lg, 12px)',
          boxShadow: 'var(--shadow-sm, 0 1px 2px rgba(0,0,0,0.06))',
          padding: '8px 0',
        }}
      >
        <Tree
          treeData={treeData}
          selectedKeys={[selectedKey]}
          expandedKeys={expandedKeys}
          onSelect={handleSelect}
          onExpand={handleExpand}
          blockNode
        />
      </div>

      {/* 右侧内容：PageCard 包裹，标题随选中节点变化 */}
      <div style={{ flex: 1, minWidth: 0, display: 'flex', flexDirection: 'column' }}>
        <PageCard
          icon={<span style={{ fontSize: 15 }}>📖</span>}
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
          contentStyle={{
            // 内容区可滚动，承载较长 markdown
            overflow: 'auto',
            padding: '16px 20px',
          }}
          style={{ flex: 1, minHeight: 0 }}
        >
          {docSource ? (
            <HelpContentRenderer source={docSource} />
          ) : (
            <Empty description="暂无帮助内容" />
          )}
        </PageCard>
      </div>
    </div>
  );
}
