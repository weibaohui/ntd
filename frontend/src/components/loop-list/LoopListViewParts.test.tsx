// LoopListViewParts.test.tsx
// ---------------------------------------------------------------------------
// 环路列表操作菜单的事件冒泡防护测试。
//
// 背景：React Portal 中的 Dropdown 菜单，合成事件会沿 React 组件树冒泡回
// 表格行（即便菜单 DOM 挂在 body），导致点击菜单项同时触发行 onClick 跳详情。
// 修复：菜单项 onClick 必须先 domEvent.stopPropagation() 再执行业务回调。
// ---------------------------------------------------------------------------

import { describe, it, expect, vi } from 'vitest';
import type { MenuProps } from 'antd';
// antd Menu 项 onClick 的事件参数类型（含 domEvent），从公开 API 推导避免深层依赖 rc-menu
type MenuInfo = Parameters<NonNullable<MenuProps['onClick']>>[0];
import { buildRowActions, loopProcessText } from './LoopListViewParts';
import type { LoopListItem } from '@/types/loop';

// 最小环路夹具：buildRowActions 只读取 status 决定「启用/暂停」文案
function makeLoop(): LoopListItem {
  return {
    id: 1,
    name: '测试环路',
    description: '',
    workspace_id: 1,
    webhook_enabled: false,
    status: 'enabled',
    tag_ids: [],
    icon: '',
    created_at: null,
    updated_at: null,
    trigger_count: 0,
    step_count: 0,
    last_execution_status: '',
    last_execution_at: null,
    pending_approval_count: 0,
  };
}

// 构造与 antd Menu onClick 签名一致的假事件（只关心 domEvent）
function makeMenuInfo() {
  const stopPropagation = vi.fn();
  const info = { domEvent: { stopPropagation } } as unknown as MenuInfo;
  return { info, stopPropagation };
}

describe('buildRowActions 冒泡防护', () => {
  it('每个菜单项 onClick 都先 stopPropagation 再调业务回调', () => {
    const callbacks = {
      onTrigger: vi.fn(),
      onDuplicate: vi.fn(),
      onDelete: vi.fn(),
      onToggleStatus: vi.fn(),
    };
    const items = buildRowActions(makeLoop(), callbacks);
    const expectedCallbacks = [
      callbacks.onTrigger,
      callbacks.onDuplicate,
      callbacks.onToggleStatus,
      callbacks.onDelete,
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

describe('工艺列统一文本', () => {
  // makeLoop 默认不含工艺字段，模拟手工环路；用例按需 spread 注入工艺字段。
  it('工艺安装的环路展示 #id-display_name-版本快照', () => {
    const loop: LoopListItem = {
      ...makeLoop(),
      process_template_id: 101,
      process_template_display_name: '标准需求交付',
      process_template_version: '1.2.0',
    };
    expect(loopProcessText(loop)).toBe('#101-标准需求交付-1.2.0');
  });

  it('display_name 缺失时回退标识名 process_template_name，版本缺失用 — 占位', () => {
    // 040 起 name 不再唯一但仍是稳定标识；display_name 为空时用它兜底展示。
    const loop: LoopListItem = {
      ...makeLoop(),
      process_template_id: 102,
      process_template_name: '4p12s-delivery',
    };
    expect(loopProcessText(loop)).toBe('#102-4p12s-delivery-—');
  });

  it('手工环路（无工艺字段）显示 -', () => {
    // 非工艺实例化的环路没有任何来源模板信息，列展示用占位符而非空文本。
    expect(loopProcessText(makeLoop())).toBe('-');
  });
});
