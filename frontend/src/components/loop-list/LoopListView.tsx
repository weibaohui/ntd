// LoopListView — 028-列表详情独立路由：环路列表 table 形态。
//
// 设计要点（028-列表详情独立路由-设计 §4.4）：
// 1. 用 Ant Design `Table` 替代原 LoopListPanel 的卡片布局，单栏宽屏展示更多列。
// 2. 点击行 / 名称链接 → 调用 onSelectLoop 由父组件跳转到 `/#/loops/:id`。
// 3. 批量操作通过 `useBatchActions` hook 复用，与 TodoListView 共享同一套 Modal。
// 4. 单行操作（删除 / 启停状态）走 Dropdown 菜单，不挤占行宽。
//    044：触发/复制菜单项已随手工环路能力下线。
// 5. 单函数 ≤ 30 行：列定义、行操作菜单、状态渲染、批量按钮已拆到 LoopListViewParts。

import { useEffect, useMemo, useState } from 'react';
import { Table } from 'antd';
// 093：本组件只消费 todo 域状态，用细粒度 useTodos 替代合并版 useApp，
// 执行态（进度/统计推送）变化不再触发本组件重渲染。
import { useTodos } from '@/hooks/useTodoContext';
import { useBatchActions } from '@/components/todo-list/useBatchActions';
import { useResizableColumns } from '@/hooks/useResizableColumns';
import type { LoopListItem } from '@/types/loop';
import {
  buildColumns,
  buildOnRow,
  BatchButton,
} from './LoopListViewParts';

interface LoopListViewProps {
  /** 已经过滤后的环路列表（搜索由父组件完成）。 */
  items: LoopListItem[];
  /** 加载态：传入 true 时 table 显示 loading 蒙层。 */
  loading: boolean;
  /** 当前行点击跳转：由父组件 pushUrl('loops', { id })。 */
  onSelectLoop: (id: number) => void;
  /** 单行删除入口（菜单「删除」）。 */
  onDelete: (loop: LoopListItem) => void;
  /** 单行切换状态入口（菜单「启用/暂停」）。 */
  onToggleStatus: (loop: LoopListItem) => void;
  /** 操作成功后的刷新回调（批量操作完后触发）。 */
  onRefresh: () => void;
}

/**
 * 环路列表 table 视图。
 *
 * 整体处理思路：
 * 1. 接收已过滤的 items，渲染 Ant Design Table。
 * 2. 内部用 useBatchActions hook 管理批量 Modal（复制移动 / 强停）。
 * 3. 选中行通过 selectedIds 受控，触发顶部批量菜单。
 * 4. 单行操作走 Dropdown 菜单：触发 / 复制 / 启停 / 删除（已拆到 buildRowActions）。
 */
export function LoopListView({
  items,
  loading,
  onSelectLoop,
  onDelete,
  onToggleStatus,
  onRefresh,
}: LoopListViewProps) {
  const { state } = useTodos();
  const workspaceId = state.selectedWorkspace;
  // 行选中态：仅持有 id 列表，由 Table 的 rowSelection 受控
  const [selectedIds, setSelectedIds] = useState<number[]>([]);

  // items 变化（刷新 / 搜索 / 单行删除）后清理已不存在的 id，避免选中计数与实际不符
  // 后续批量操作也不会带上已失效的 id
  useEffect(() => {
    setSelectedIds(prev => prev.filter(id => items.some(item => item.id === id)));
  }, [items]);

  // 复用共享 hook：批量 Modal + 菜单项一次到位（loop 模式）
  const { batchActions, modals } = useBatchActions({
    mode: 'loop',
    selectedWorkspace: workspaceId,
    onRefreshLoops: onRefresh,
    onClearSelection: () => setSelectedIds([]),
  });

  // 列定义：抽为独立 useMemo，避免每次渲染重建（已拆到 buildColumns）
  const rawColumns = useMemo(() => buildColumns({
    onSelectLoop,
    onDelete,
    onToggleStatus,
  }), [onSelectLoop, onDelete, onToggleStatus]);
  // 054：注入可拖拽列宽 + 受控排序 + localStorage 持久化。
  const { columns, tableProps } = useResizableColumns('loops', rawColumns);

  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%', overflow: 'hidden' }}>
      {/* 顶部工具栏：批量操作按钮（刷新在 PageCard 顶部 header，避免重复） */}
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          gap: 8,
          padding: '6px 12px',
          borderBottom: '1px solid var(--color-border-light)',
          flexShrink: 0,
        }}
      >
        <BatchButton selectedIds={selectedIds} batchActions={batchActions} />
        <div style={{ flex: 1 }} />
        <span
          style={{
            fontSize: 12,
            color: 'var(--color-text-tertiary)',
          }}
        >
          已选 {selectedIds.length} 项
        </span>
      </div>

      {/* table 主体 */}
      <div style={{ flex: 1, minHeight: 0, overflow: 'auto' }}>
        <Table<LoopListItem>
          rowKey="id"
          columns={columns}
          dataSource={items}
          loading={loading}
          size="small"
          // 054：scroll.x 由 hook 动态计算（列宽求和），替代原硬编码 1360
          {...tableProps}
          pagination={{
            pageSize: 20,
            showSizeChanger: true,
            pageSizeOptions: ['20', '50', '100'],
            showTotal: (total) => `共 ${total} 个环路`,
          }}
          rowSelection={{
            selectedRowKeys: selectedIds,
            onChange: (keys) => setSelectedIds(keys as number[]),
          }}
          onRow={buildOnRow(onSelectLoop)}
        />
      </div>

      {/* 批量操作 Modal（由 useBatchActions 集中渲染） */}
      {modals}
    </div>
  );
}