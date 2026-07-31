/**
 * tablePrefs 单元测试。
 * 覆盖：白名单校验、JSON 容错、字段白名单、正常读写、清除。
 */
import { describe, it, expect, beforeEach } from 'vitest';
import { getTablePrefs, setTablePrefs, resetTablePrefs } from './tablePrefs';

beforeEach(() => {
  localStorage.clear();
});

describe('getTablePrefs', () => {
  it('无记录时返回 null', () => {
    expect(getTablePrefs('todos')).toBeNull();
  });

  it('非法 key 返回 null', () => {
    localStorage.setItem('ntd_table_prefs:evil', '{}');
    expect(getTablePrefs('evil')).toBeNull();
  });

  it('JSON 损坏返回 null（不抛异常）', () => {
    localStorage.setItem('ntd_table_prefs:todos', '{invalid');
    expect(getTablePrefs('todos')).toBeNull();
  });

  it('非对象 JSON 返回 null', () => {
    localStorage.setItem('ntd_table_prefs:todos', '"string"');
    expect(getTablePrefs('todos')).toBeNull();
    localStorage.setItem('ntd_table_prefs:todos', '42');
    expect(getTablePrefs('todos')).toBeNull();
  });

  it('widths 过滤非数字值', () => {
    localStorage.setItem(
      'ntd_table_prefs:todos',
      JSON.stringify({ widths: { id: 80, title: 'abc', bad: -1 }, sort: { field: 'id', order: 'descend' } })
    );
    const prefs = getTablePrefs('todos');
    expect(prefs?.widths).toEqual({ id: 80 });
  });

  it('非法 order 回退 null', () => {
    localStorage.setItem(
      'ntd_table_prefs:todos',
      JSON.stringify({ widths: {}, sort: { field: 'id', order: 'invalid' } })
    );
    expect(getTablePrefs('todos')?.sort.order).toBeNull();
  });

  it('缺失 sort.field 回退 id', () => {
    localStorage.setItem(
      'ntd_table_prefs:todos',
      JSON.stringify({ widths: {}, sort: { order: 'ascend' } })
    );
    expect(getTablePrefs('todos')?.sort.field).toBe('id');
  });
});

describe('setTablePrefs', () => {
  it('正常写入后可读回', () => {
    setTablePrefs('tasks', { widths: { id: 60 }, sort: { field: 'id', order: 'ascend' } });
    expect(getTablePrefs('tasks')?.widths.id).toBe(60);
  });

  it('非法 key 静默拒绝', () => {
    expect(() =>
      setTablePrefs('evil', { widths: {}, sort: { field: 'id', order: null } })
    ).not.toThrow();
    expect(localStorage.getItem('ntd_table_prefs:evil')).toBeNull();
  });
});

describe('resetTablePrefs', () => {
  it('清除后返回 null', () => {
    setTablePrefs('loops', { widths: { id: 70 }, sort: { field: 'id', order: 'descend' } });
    resetTablePrefs('loops');
    expect(getTablePrefs('loops')).toBeNull();
  });

  it('非法 key 不抛异常', () => {
    expect(() => resetTablePrefs('evil')).not.toThrow();
  });
});
