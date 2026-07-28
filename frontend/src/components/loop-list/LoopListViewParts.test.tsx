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
import { buildRowActions } from './LoopListViewParts';
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
