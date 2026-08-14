// CollapsiblePropertyPanel 单测：
//  ① getCollapsedPanelTitle 纯函数——工艺/阶段/环节/悬空兜底四种路由。
//  ② 渲染——展开态显示「收起属性面板」按钮，点击后切到收起态显示「展开属性面板」。
import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { CollapsiblePropertyPanel, getCollapsedPanelTitle } from './CollapsiblePropertyPanel';
import type { ProcessDefinition } from '@/types/process';

// 最小构造：phase 含 link，覆盖三种命中分支。
const DEF = {
  phases: [{ id: 'p1', links: [{ id: 'l1' }] }],
} as unknown as ProcessDefinition;

describe('getCollapsedPanelTitle — 表单路由标题', () => {
  it('test_getCollapsedPanelTitle_未选节点返回工艺属性', () => {
    expect(getCollapsedPanelTitle(DEF, null)).toBe('工艺属性');
  });

  it('test_getCollapsedPanelTitle_definition为null兜底工艺属性', () => {
    expect(getCollapsedPanelTitle(null, 'p1')).toBe('工艺属性');
  });

  it('test_getCollapsedPanelTitle_命中phase返回阶段属性', () => {
    expect(getCollapsedPanelTitle(DEF, 'p1')).toBe('阶段属性');
  });

  it('test_getCollapsedPanelTitle_命中link返回环节属性', () => {
    expect(getCollapsedPanelTitle(DEF, 'l1')).toBe('环节属性');
  });

  it('test_getCollapsedPanelTitle_悬空引用兜底工艺属性', () => {
    // 节点已删但 selectedNodeId 未清：不应误判为阶段/环节。
    expect(getCollapsedPanelTitle(DEF, 'ghost')).toBe('工艺属性');
  });
});

describe('CollapsiblePropertyPanel — 收缩/展开切换', () => {
  it('test_展开态点收起后切到收起态显示展开按钮', () => {
    // definition=null 让面板体走 Empty 分支，隔离掉 ProcessPropertyPanel 的渲染依赖，
    // 只验收缩栏交互。标题在 null 下兜底为「工艺属性」。
    render(
      <CollapsiblePropertyPanel definition={null} selectedNodeId={null} onDefinitionChange={vi.fn()} />,
    );

    // 展开态：工具条标题 + 「收起属性面板」按钮可见。
    expect(screen.getByText('工艺属性')).toBeInTheDocument();
    const collapseBtn = screen.getByRole('button', { name: '收起属性面板' });
    fireEvent.click(collapseBtn);

    // 收起态：切到「展开属性面板」按钮（竖排标题同样是工艺属性，但按钮 aria-label 区分）。
    expect(screen.getByRole('button', { name: '展开属性面板' })).toBeInTheDocument();

    // 再点展开，回到展开态。
    fireEvent.click(screen.getByRole('button', { name: '展开属性面板' }));
    expect(screen.getByRole('button', { name: '收起属性面板' })).toBeInTheDocument();
  });
});
