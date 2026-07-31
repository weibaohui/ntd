/**
 * useResizableColumns 单元测试。
 *
 * 验证：
 * 1. 默认排序兜底（无 localStorage 时 ID 倒序）
 * 2. localStorage 中排序偏好被正确注入列定义（sortOrder）
 * 3. localStorage 中列宽偏好被正确注入列定义（width）
 * 4. scroll.x 动态计算（列宽求和）
 * 5. tableProps 结构完整（components / scroll / onChange）
 */
import { describe, it, expect, beforeEach } from 'vitest';
import { renderHook } from '@testing-library/react';
import type { ColumnsType } from 'antd/es/table';
import { useResizableColumns, compareValues, makeSorter } from './useResizableColumns';
import { setTablePrefs } from '@/components/common/tablePrefs';

interface TestItem {
  id: number;
  name: string;
}

/** 最小列定义：ID + 名称，供测试注入。 */
const RAW_COLUMNS: ColumnsType<TestItem> = [
  { title: 'ID', dataIndex: 'id', key: 'id', width: 70, sorter: makeSorter<TestItem>('id') },
  { title: '名称', dataIndex: 'name', key: 'name', width: 200, sorter: makeSorter<TestItem>('name') },
];

beforeEach(() => {
  localStorage.clear();
});

describe('compareValues', () => {
  it('数字比较', () => {
    expect(compareValues(1, 2)).toBeLessThan(0);
    expect(compareValues(2, 1)).toBeGreaterThan(0);
    expect(compareValues(1, 1)).toBe(0);
  });

  it('字符串比较', () => {
    expect(compareValues('a', 'b')).toBeLessThan(0);
    expect(compareValues('b', 'a')).toBeGreaterThan(0);
  });

  it('null/undefined 转为空字符串比较', () => {
    expect(compareValues(null, 'a')).toBeLessThan(0);
    expect(compareValues(undefined, '')).toBe(0);
  });
});

describe('makeSorter', () => {
  it('按数字字段比较（委托 compareValues 数值分支）', () => {
    const sorter = makeSorter<TestItem>('id');
    expect(sorter({ id: 1, name: 'a' }, { id: 2, name: 'b' })).toBeLessThan(0);
    expect(sorter({ id: 2, name: 'a' }, { id: 1, name: 'b' })).toBeGreaterThan(0);
  });

  it('按字符串字段比较（委托 compareValues localeCompare 分支）', () => {
    const sorter = makeSorter<TestItem>('name');
    expect(sorter({ id: 1, name: 'a' }, { id: 2, name: 'b' })).toBeLessThan(0);
    expect(sorter({ id: 2, name: 'b' }, { id: 1, name: 'a' })).toBeGreaterThan(0);
  });

  it('两元素同字段相等时返回 0', () => {
    const sorter = makeSorter<TestItem>('id');
    expect(sorter({ id: 5, name: 'a' }, { id: 5, name: 'b' })).toBe(0);
  });

  it('字段缺失（undefined）走空字符串兜底，不抛错', () => {
    interface WithOpt { id: number; tag?: string }
    const sorter = makeSorter<WithOpt>('tag');
    // 两边 tag 均缺失：都转 '' → 比较结果为 0
    expect(sorter({ id: 1 }, { id: 2 })).toBe(0);
    // 一方有值一方缺失：'a' 排在 '' 之后 → 正数
    expect(sorter({ id: 1, tag: 'a' }, { id: 2 })).toBeGreaterThan(0);
  });
});

describe('useResizableColumns', () => {
  it('无 localStorage 时默认按 ID 倒序', () => {
    const { result } = renderHook(() => useResizableColumns('todos', RAW_COLUMNS));
    const idCol = result.current.columns.find(c => c.key === 'id');
    // 默认 ID 列 sortOrder = descend
    expect((idCol as Record<string, unknown>)?.sortOrder).toBe('descend');
    // 其他列 sortOrder = null
    const nameCol = result.current.columns.find(c => c.key === 'name');
    expect((nameCol as Record<string, unknown>)?.sortOrder).toBeNull();
  });

  it('localStorage 中的排序偏好被注入列定义', () => {
    setTablePrefs('todos', { widths: {}, sort: { field: 'name', order: 'ascend' } });
    const { result } = renderHook(() => useResizableColumns('todos', RAW_COLUMNS));
    const nameCol = result.current.columns.find(c => c.key === 'name');
    expect((nameCol as Record<string, unknown>)?.sortOrder).toBe('ascend');
    const idCol = result.current.columns.find(c => c.key === 'id');
    expect((idCol as Record<string, unknown>)?.sortOrder).toBeNull();
  });

  it('localStorage 中的列宽偏好覆盖默认宽度', () => {
    setTablePrefs('todos', { widths: { id: 150 }, sort: { field: 'id', order: 'descend' } });
    const { result } = renderHook(() => useResizableColumns('todos', RAW_COLUMNS));
    const idCol = result.current.columns.find(c => c.key === 'id');
    expect(idCol?.width).toBe(150);
    // 未保存的列保持默认宽度
    const nameCol = result.current.columns.find(c => c.key === 'name');
    expect(nameCol?.width).toBe(200);
  });

  it('scroll.x = 列宽求和', () => {
    const { result } = renderHook(() => useResizableColumns('todos', RAW_COLUMNS));
    // 默认宽度 70 + 200 = 270
    expect(result.current.tableProps.scroll?.x).toBe(270);
  });

  it('tableProps 包含 components / onChange', () => {
    const { result } = renderHook(() => useResizableColumns('todos', RAW_COLUMNS));
    expect(result.current.tableProps.components).toBeDefined();
    expect(result.current.tableProps.onChange).toBeDefined();
  });

  it('每列都有 onHeaderCell（注入 width + onResize）', () => {
    const { result } = renderHook(() => useResizableColumns('todos', RAW_COLUMNS));
    for (const col of result.current.columns) {
      expect(col.onHeaderCell).toBeDefined();
      // onHeaderCell 的参数是列定义本身（antd 内部透传），不是行数据
      const headerProps = col.onHeaderCell?.(col as never);
      expect(headerProps).toHaveProperty('width');
      expect(headerProps).toHaveProperty('onResize');
    }
  });
});
