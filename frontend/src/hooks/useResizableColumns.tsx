/**
 * useResizableColumns — 给 antd Table 列定义注入「可拖拽列宽 + 受控排序 + 持久化」。
 *
 * 功能：
 * 1. 列宽注入：为每列添加 `onHeaderCell`（透传 width + onResize/onResizeEnd），
 *    并返回 `components` 供 Table 替换表头 cell 为 ResizableTitle。
 * 2. 受控排序：为可排序列注入 `sortOrder`（由 state 驱动），
 *    Table onChange 时提取排序状态并写 localStorage。
 *    排序本身由 antd Table 内置机制完成（列定义中的 `sorter` 比较函数）。
 * 3. 动态 scroll.x：当前列宽求和，替代硬编码的固定值。
 * 4. 持久化：列宽拖拽结束 / 排序切换时写 localStorage（tablePrefs 纯函数层）。
 *
 * 用法：
 *   const { columns, components, tableProps } = useResizableColumns('todos', rawColumns);
 *   <Table columns={columns} {...tableProps} dataSource={items} />
 */

import { useCallback, useMemo, useState } from 'react';
import type { ColumnsType } from 'antd/es/table';
import type { TableProps } from 'antd';
import { ResizableTitle } from '@/components/common/ResizableTitle';
import { getTablePrefs, setTablePrefs, type SortState } from '@/components/common/tablePrefs';

/** 默认排序兜底：localStorage 无记录时按 ID 倒序。 */
const DEFAULT_SORT: SortState = { field: 'id', order: 'descend' };

/**
 * 通用排序比较器：数字按数值、日期按时间戳、其余按中文字符串比较。
 * 供列定义的 `sorter` 字段直接引用，避免每个列表重复写比较逻辑。
 */
export function compareValues(a: unknown, b: unknown): number {
  if (typeof a === 'number' && typeof b === 'number') return a - b;
  if (a instanceof Date && b instanceof Date) return a.getTime() - b.getTime();
  const sa = String(a ?? '');
  const sb = String(b ?? '');
  return sa.localeCompare(sb, 'zh-CN');
}

/**
 * 为列定义生成 `sorter` 比较函数。
 * 用法：在列定义中加 `sorter: (a, b) => compareValues(a.title, b.title)`。
 */
export function makeSorter<T>(field: keyof T) {
  return (a: T, b: T) => compareValues(a[field], b[field]);
}

export function useResizableColumns<T>(
  tableKey: string,
  rawColumns: ColumnsType<T>,
) {
  // 初始化：从 localStorage 读偏好，无记录则用默认值。
  const [prefs, setPrefs] = useState(() => {
    const saved = getTablePrefs(tableKey);
    return {
      widths: saved?.widths ?? {},
      sort: saved?.sort ?? DEFAULT_SORT,
    };
  });

  /** 拖拽中更新列宽 state（高频，不写存储）。 */
  const handleResize = useCallback((colKey: string, width: number) => {
    setPrefs(prev => ({ ...prev, widths: { ...prev.widths, [colKey]: width } }));
  }, []);

  /** 拖拽结束：写 localStorage + 更新 state。 */
  const handleResizeEnd = useCallback(
    (colKey: string, width: number) => {
      setPrefs(prev => {
        const next = { ...prev, widths: { ...prev.widths, [colKey]: width } };
        setTablePrefs(tableKey, next);
        return next;
      });
    },
    [tableKey],
  );

  /** Table onChange：提取排序状态并持久化。 */
  const handleTableChange = useCallback<
    NonNullable<TableProps<T>['onChange']>
  >(
    (_pagination, _filters, sorter) => {
      // antd 单排序时 sorter 为对象；多排序为数组（本设计只用单排序）。
      const s = Array.isArray(sorter) ? sorter[0] : sorter;
      const nextSort: SortState = s?.field
        ? { field: String(s.field), order: s.order ?? null }
        : DEFAULT_SORT;
      setPrefs(prev => {
        const next = { ...prev, sort: nextSort };
        setTablePrefs(tableKey, next);
        return next;
      });
    },
    [tableKey],
  );

  /** 注入列宽与排序后的列定义。 */
  const columns = useMemo<ColumnsType<T>>(() => {
    return rawColumns.map(col => {
      // ColumnGroupType 无 dataIndex（是分组容器）；本项目全部用扁平 ColumnType，
      // 用 'dataIndex' in col 收窄类型，TypeScript 才允许访问。
      const dataIndex = 'dataIndex' in col ? col.dataIndex : undefined;
      const key = String(col.key ?? dataIndex ?? '');
      const defaultWidth = typeof col.width === 'number' ? col.width : 120;
      const currentWidth = prefs.widths[key] ?? defaultWidth;
      // 操作列（key 含 action 或无 dataIndex）不注入拖拽手柄，保持固定宽度。
      const isActionCol = !dataIndex || key.toLowerCase().includes('action');
      return {
        ...col,
        width: currentWidth,
        onHeaderCell: () => ({
          width: currentWidth,
          // 透传 fixed 标记：ResizableTitle 据此决定是否覆盖 position（固定列已是 sticky）
          fixed: 'fixed' in col ? col.fixed : undefined,
          onResize: isActionCol ? undefined : (w: number) => handleResize(key, w),
          onResizeEnd: isActionCol ? undefined : (w: number) => handleResizeEnd(key, w),
        }),
        // 可排序列：注入受控 sortOrder；无 sorter 的列不动。
        ...(col.sorter
          ? { sortOrder: (prefs.sort.field === key ? prefs.sort.order : null) as 'ascend' | 'descend' | null }
          : {}),
      };
    });
  }, [rawColumns, prefs, handleResize, handleResizeEnd]);

  /** 动态 scroll.x：当前列宽求和。 */
  const scrollX = useMemo(() => {
    return columns.reduce((acc, col) => acc + (typeof col.width === 'number' ? col.width : 0), 0);
  }, [columns]);

  return {
    columns,
    tableProps: {
      components: { header: { cell: ResizableTitle } },
      scroll: { x: scrollX },
      onChange: handleTableChange,
    } satisfies Partial<TableProps<T>>,
  };
}
