// TodoListView.test.tsx
// ---------------------------------------------------------------------------
// 事项列表操作菜单的事件冒泡防护测试。
//
// 背景：React Portal 中的 Dropdown 菜单，合成事件会沿 React 组件树冒泡回
// 表格行（即便菜单 DOM 挂在 body），导致点击菜单项同时触发行 onClick 跳详情。
// 修复：菜单项 onClick 必须先 domEvent.stopPropagation() 再执行业务回调。
// ---------------------------------------------------------------------------

import { describe, it, expect, vi } from 'vitest';
import type { MenuProps } from 'antd';
import { renderToStaticMarkup } from 'react-dom/server';
// antd Menu 项 onClick 的事件参数类型（含 domEvent），从公开 API 推导避免深层依赖 rc-menu
type MenuInfo = Parameters<NonNullable<MenuProps['onClick']>>[0];
import { buildRowActionItems, renderLoopColumn, renderProcessColumn } from './TodoListView';
import type { LoopRefSummary, TodoCenterItem } from '@/types';

// 最小事项夹具：buildRowActionItems 不读字段，仅透传 record 给回调
const todo = { id: 1, title: '测试事项' } as unknown as TodoCenterItem;

// 构造与 antd Menu onClick 签名一致的假事件（只关心 domEvent）
function makeMenuInfo() {
  const stopPropagation = vi.fn();
  const info = { domEvent: { stopPropagation } } as unknown as MenuInfo;
  return { info, stopPropagation };
}

describe('buildRowActionItems 冒泡防护', () => {
  it('每个菜单项 onClick 都先 stopPropagation 再调业务回调', () => {
    const callbacks = {
      onExecuteTodo: vi.fn(),
      onExecuteWithArgs: vi.fn(),
      onEditTodo: vi.fn(),
      onDeleteTodo: vi.fn(),
    };
    const items = buildRowActionItems(todo, callbacks);
    const expectedCallbacks = [
      callbacks.onExecuteTodo,
      callbacks.onExecuteWithArgs,
      callbacks.onEditTodo,
      callbacks.onDeleteTodo,
    ];

    // 逐项验证：4 个菜单项都必须阻止冒泡，且业务回调仍被调用
    expect(items.length).toBe(4);
    items.forEach((item, i) => {
      const { info, stopPropagation } = makeMenuInfo();
      item.onClick?.(info);
      expect(stopPropagation).toHaveBeenCalledTimes(1);
      expect(expectedCallbacks[i]).toHaveBeenCalledTimes(1);
    });
  });
});

// ── 环路列 / 工艺列 渲染测试 ───────────────────────

// 把 ReactNode 渲染成字符串，便于断言 #ID 名称 格式是否落地
function renderText(node: unknown): string {
  return renderToStaticMarkup(node as React.ReactElement);
}

describe('renderLoopColumn / renderProcessColumn', () => {
  const refs: LoopRefSummary[] = [
    { loop_id: 17, loop_name: '门禁测试', process_template_id: 101, process_template_name: '标准工艺' },
    { loop_id: 18, loop_name: '环路2', process_template_id: 101, process_template_name: '标准工艺' },
    { loop_id: 19, loop_name: '裸环路' },
  ];

  it('renderLoopColumn_usesHashIdNameFormat', () => {
    const html = renderText(renderLoopColumn(refs));
    // 三条环路都应是 #ID 名称
    expect(html).toContain('#17 门禁测试');
    expect(html).toContain('#18 环路2');
    expect(html).toContain('#19 裸环路');
  });

  it('renderLoopColumn_emptyReturnsDash', () => {
    expect(renderText(renderLoopColumn(undefined))).toBe('-');
    expect(renderText(renderLoopColumn([]))).toBe('-');
  });

  it('renderProcessColumn_dedupesByTemplateIdAndUsesHashIdName', () => {
    const html = renderText(renderProcessColumn(refs));
    // 模板 101 被两个环路引用，应只出现一次
    expect(html).toContain('#101 标准工艺');
    expect((html.match(/#101 标准工艺/g) || []).length).toBe(1);
    // 裸环路无模板，不应出现其 loop 信息
    expect(html).not.toContain('裸环路');
  });

  it('renderProcessColumn_noTemplateReturnsDash', () => {
    // 所有环路都没绑模板 → 工艺列显示 -
    expect(renderText(renderProcessColumn([{ loop_id: 1, loop_name: 'L' }]))).toBe('-');
  });
});
