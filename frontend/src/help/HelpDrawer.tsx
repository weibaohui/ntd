// 帮助抽屉容器：左树右内容。
//
// 设计要点：
// 1. 右侧 Drawer，width 560，与现有 navDrawerOpen 模式一致。
// 2. 左侧 Antd Tree 展示「页面 → 功能点」两级，自动选中当前页面总览。
// 3. 右侧用 HelpContentRenderer 渲染选中节点的 md。
// 4. 树形数据从 HELP_PAGES 派生，节点 key 编码：页面='p:<pageId>', 功能点='f:<pageId>/<featureId>'。

import { useMemo, useState, useEffect } from 'react';
import { Drawer, Tree, Empty } from 'antd';
import type { TreeDataNode } from 'antd';
import { HELP_PAGES } from './index';
import { findHelpPage, loadHelpDoc, useDefaultPageId } from './useHelpContent';
import { HelpContentRenderer } from './HelpContentRenderer';
import type { View } from '@/hooks/useViewState';

interface HelpDrawerProps {
  /** 是否展开。 */
  open: boolean;
  /** 关闭抽屉的回调。 */
  onClose: () => void;
  /** 当前所在视图，用于树形默认选中。 */
  activeView: View;
  /** 是否处于详情形态（todoDetailId/loopDetailId/taskDetailId != null）。 */
  hasDetail: boolean;
  /** 是否移动端：移动端抽屉全屏，树形与内容上下排列。 */
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

/** 帮助抽屉容器。 */
export function HelpDrawer({ open, onClose, activeView, hasDetail, isMobile }: HelpDrawerProps) {
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

  // 解析当前选中节点对应的 md 文件名
  const docFile = useMemo(() => resolveDocFile(selectedKey), [selectedKey]);
  const docSource = useMemo(() => loadHelpDoc(docFile), [docFile]);

  // 树节点选中回调。
  // 选中页面节点（'p:' 前缀）时同步将其并入 expandedKeys：antd Tree 默认只有点 switcher
  // 小箭头才展开子节点，但帮助树是菜单语义，用户习惯点标题文字展开子菜单（NTD-011）。
  // 收起动作仍交给 switcher 箭头承担，避免「点已选中节点标题」与「收起」产生冲突。
  function handleSelect(keys: React.Key[]) {
    if (keys.length === 0) return;
    const key = String(keys[0]);
    setSelectedKey(key);
    if (key.startsWith('p:') && !expandedKeys.includes(key)) {
      setExpandedKeys([...expandedKeys, key]);
    }
  }

  // 树节点展开回调
  function handleExpand(keys: React.Key[]) {
    setExpandedKeys(keys.map(String));
  }

  return (
    <Drawer
      title="帮助"
      placement="right"
      open={open}
      onClose={onClose}
      width={isMobile ? '100%' : 560}
      styles={{ body: { padding: 0 } }}
    >
      <div style={{ display: 'flex', flexDirection: isMobile ? 'column' : 'row', height: '100%' }}>
        {/* 左侧树（移动端在上） */}
        <div
          style={{
            width: isMobile ? '100%' : 180,
            maxHeight: isMobile ? 200 : undefined,
            borderRight: isMobile ? undefined : '1px solid var(--ant-color-border, #f0f0f0)',
            borderBottom: isMobile ? '1px solid var(--ant-color-border, #f0f0f0)' : undefined,
            overflow: 'auto',
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
        {/* 右侧内容 */}
        <div style={{ flex: 1, overflow: 'auto', padding: '16px 20px' }}>
          {docSource ? (
            <HelpContentRenderer source={docSource} />
          ) : (
            <Empty description="暂无帮助内容" />
          )}
        </div>
      </div>
    </Drawer>
  );
}
