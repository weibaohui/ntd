// 帮助系统的类型契约。
// 注册表 HELP_PAGES 由 help/index.ts 提供，本文件只定义形状，
// 便于 useHelpContent / HelpDrawer 等多处共享同一份类型。

/**
 * 单个功能点的注册信息。
 *
 * 一个「功能点」对应页面上一个可见的按钮/操作/区块，
 * 是帮助树形结构的二级节点。
 */
interface HelpFeature {
  /** 功能点 id，在所属页面内唯一，如 "create-todo"。 */
  id: string;
  /** 中文功能名，显示在树形二级节点。 */
  title: string;
  /** 该功能点帮助 md 的文件名（相对 help/pages/），如 "todos-list_create-todo.md"。 */
  docFile: string;
}

/**
 * 单个页面的注册信息。
 *
 * 一个「页面」对应 LeftRail 的一个导航视图，
 * 是帮助树形结构的一级节点，下挂多个功能点。
 */
export interface HelpPage {
  /** 页面 id，对应 viewToPageId 派生值，如 "todos-list"。 */
  pageId: string;
  /** 页面中文名，显示在树形一级节点。 */
  title: string;
  /** 页面级总览 md 的文件名，如 "todos-list.md"。 */
  overviewDoc: string;
  /** 该页面下的功能点列表。 */
  features: HelpFeature[];
}
