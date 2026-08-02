// decideTreeSelect 单元测试（PR #978 评审意见 #3：公开逻辑需有单元测试，Playwright 不替代单测）。
//
// 覆盖场景：
// - 正常路径：选中未展开页面节点 → 选中 + 追加展开
// - 边界：keys 为空（反选）→ 不改动状态；页面节点已展开 → 不重复追加
// - 边界：选中功能点叶子节点（'f:' 前缀）→ 仅选中不展开；数字 key 统一转 string
import { describe, it, expect } from 'vitest';
import { decideTreeSelect } from './helpTreeSelect';

describe('decideTreeSelect', () => {
  it('test_decideTreeSelect_选中未展开页面节点_返回选中并追加展开', () => {
    // 典型路径：用户点击「仪表盘」标题，期望同时选中并展开其子菜单
    const d = decideTreeSelect(['p:dashboard'], ['p:todos-list']);
    expect(d.selectedKey).toBe('p:dashboard');
    expect(d.expandKey).toBe('p:dashboard');
  });

  it('test_decideTreeSelect_keys为空_返回null表示不改动状态', () => {
    // 点击已选中节点时 antd Tree 传空数组（反选）；帮助树要求始终有选中节点
    const d = decideTreeSelect([], ['p:todos-list']);
    expect(d.selectedKey).toBeNull();
    expect(d.expandKey).toBeNull();
  });

  it('test_decideTreeSelect_页面节点已展开_不重复追加展开', () => {
    // 已展开节点再次选中只更新选中态，避免 expandedKeys 出现重复 key
    const d = decideTreeSelect(['p:todos-list'], ['p:todos-list']);
    expect(d.selectedKey).toBe('p:todos-list');
    expect(d.expandKey).toBeNull();
  });

  it('test_decideTreeSelect_选中功能点叶子节点_仅选中不展开', () => {
    // 'f:' 前缀是功能点叶子，无子节点可展开
    const d = decideTreeSelect(['f:todos-list/todo-list-create'], []);
    expect(d.selectedKey).toBe('f:todos-list/todo-list-create');
    expect(d.expandKey).toBeNull();
  });

  it('test_decideTreeSelect_数字key_统一转字符串', () => {
    // React.Key 允许 number：防御性转换，避免后续 startsWith 抛类型错误
    const d = decideTreeSelect([123], []);
    expect(d.selectedKey).toBe('123');
    expect(d.expandKey).toBeNull();
  });
});
