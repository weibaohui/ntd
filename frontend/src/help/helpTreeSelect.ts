// 帮助树选中决策（纯函数模块）。
//
// 独立成文件的原因：HelpDrawer.tsx 的导入链包含 XMarkdown / mermaid 等重型浏览器依赖，
// 在 jsdom 单测中直接 import 组件会引入大量无关初始化；把选中决策抽成纯函数后，
// 可用 vitest 零渲染地完成单元测试（PR #978 评审意见 #3：公开逻辑需有单元测试）。

/** 树节点选中决策结果。 */
export interface TreeSelectDecision {
  /** 应选中的节点 key；输入 keys 为空时为 null，表示「保持原选中不变」。 */
  selectedKey: string | null;
  /** 应追加进 expandedKeys 的页面节点 key；无需追加时为 null。 */
  expandKey: string | null;
}

/**
 * 根据 antd Tree 的 onSelect 入参计算选中决策。
 *
 * 规则：
 * 1. keys 为空（点击已选中节点触发反选）→ 不改动任何状态；
 * 2. 选中页面节点（'p:' 前缀）且尚未展开 → 追加到 expandedKeys，实现「点标题即展开」；
 * 3. 选中功能点节点（'f:' 前缀）或已展开的页面节点 → 仅更新选中态。
 *
 * @param keys antd Tree onSelect 的第一个参数（选中节点 key 数组）
 * @param expandedKeys 当前已展开的节点 key 数组
 * @returns 选中决策
 */
export function decideTreeSelect(keys: React.Key[], expandedKeys: string[]): TreeSelectDecision {
  // 空数组 = 反选手势：帮助树要求始终有一个选中节点，直接忽略
  if (keys.length === 0) {
    return { selectedKey: null, expandKey: null };
  }
  // antd Tree 单选模式下 keys[0] 即目标节点；React.Key 可能是 number，统一转 string
  const key = String(keys[0]);
  // 只有未展开的页面节点才需要追加展开；功能点是叶子、已展开页面节点追加也无意义
  const expandKey = key.startsWith('p:') && !expandedKeys.includes(key) ? key : null;
  return { selectedKey: key, expandKey };
}
