import { describe, it, expect, vi } from 'vitest';
import { changeStatusMenuItem } from './TodoCenterCard';
import * as db from '@/utils/database';
import type { TodoCenterItem } from '@/types';

// 只测 changeStatusMenuItem 纯逻辑（不渲染组件，不依赖 jsdom localStorage）。
vi.mock('@/utils/database');

const item = { id: 5, workspace_id: 1 } as unknown as TodoCenterItem;

describe('changeStatusMenuItem', () => {
  it('提供 4 个状态子项（待办/进行中/已完成/失败）', () => {
    const runMutation = vi.fn();
    const result = changeStatusMenuItem(item, 1, runMutation) as unknown as {
      key: string;
      children: { key: string; label: string; onClick: () => void }[];
    };
    expect(result.key).toBe('change_status');
    expect(result.children).toHaveLength(4);
    expect(result.children.map(c => c.label)).toEqual(['待办', '进行中', '已完成', '失败']);
  });

  it('点击「已完成」→ 经 runMutation 触发 updateTodoStatus(1, 5, "completed")', () => {
    // runMutation mock：执行传入的 fn，让其中的 updateTodoStatus 真正被调用
    const runMutation = vi.fn((_label, fn) => fn());
    const result = changeStatusMenuItem(item, 1, runMutation) as unknown as {
      children: { onClick: () => void }[];
    };
    result.children[2].onClick();
    expect(db.updateTodoStatus).toHaveBeenCalledWith(1, 5, 'completed');
  });
});
