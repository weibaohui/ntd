// 帮助内容动态加载 Hook。
//
// 设计要点：
// 1. 用 Vite 的 import.meta.glob 在构建时把 help/pages/*.md 以 raw string 打进 bundle，
//    运行时按文件名取出，无需 fetch，也不依赖 server 返回 md。
// 2. md 文件仍在源码树里，AI 可直接 read_file/write_file/edit_file 编辑。
// 3. 视图 → pageId 的映射也集中在这里，避免 HelpDrawer 重复实现。

import { useMemo } from 'react';
import type { View } from '@/hooks/useViewState';
import { HELP_PAGES } from './index';
import type { HelpPage } from './types';

// 构建时把所有 md 文件以 raw string 形式打进去。
// 设计意图：运行时无需 fetch 或依赖 server 返回 md（NTD-011）。
const allDocs = import.meta.glob('./pages/*.md', {
  query: '?raw',                // Vite 返回原始 Markdown 文本字符串（不解析）。
  import: 'default',            // 取模块默认导出——即 ?raw 注入的 raw string。
  eager: true,                  // 构建时内联：默认懒加载会返回 () => Promise<string> 函数；
                                // 漏传则 loadHelpDoc 返回函数，XMarkdown 拒绝渲染。
                                // 代价：约 393KB 原始文本打入主 chunk（gzip ~100KB），
                                // 属预期取舍——帮助文档无敏感信息且首次打开即用，避免懒加载闪烁。
}) as unknown as Record<string, string>;
// 文档 key 缺失时的边界行为：由 loadHelpDoc 的 allDocs[key] ?? '' 兜底，
// 返回空串 → resolveDocFile 找不到文件 → HelpContentRenderer 走 Empty 分支（非崩溃）。

/**
 * 根据 md 文件名取出内容，找不到返回空串。
 *
 * @param docFile 文件名（不含路径），如 "todos-list.md"
 * @returns md 源码，找不到时返回 ''
 */
export function loadHelpDoc(docFile: string): string {
  // allDocs 的 key 形如 './pages/todos-list.md'
  const key = `./pages/${docFile}`;
  return allDocs[key] ?? '';
}

/**
 * 视图 → 帮助页面 pageId 的映射。
 *
 * todos/loops/tasks 都有「列表」和「详情」两种形态，拆成两个 pageId。
 * 详情形态的 pageId 加 '-detail' 后缀。
 *
 * @param view 当前视图
 * @param hasDetail 是否处于详情形态（todoDetailId/loopDetailId/taskDetailId != null）
 * @returns 对应的 pageId，找不到时返回 '_overview'
 */
export function viewToPageId(view: View, hasDetail: boolean): string {
  // 列表/详情双形态视图：详情加 '-detail' 后缀
  if (view === 'todos') return hasDetail ? 'todos-detail' : 'todos-list';
  if (view === 'loops') return hasDetail ? 'loops-detail' : 'loops-list';
  if (view === 'tasks') return hasDetail ? 'tasks-detail' : 'tasks-list';
  // settings 下有多个子面板，各自对应一个 pageId
  if (view === 'skills') return 'settings-skills';
  if (view === 'experts') return 'settings-experts';
  if (view === 'executors') return 'settings-executors';
  if (view === 'bots') return 'settings-bots';
  if (view === 'workspaces') return 'settings-pd';
  if (view === 'settings') return 'settings-more';
  // 单形态视图直接用 view 名作为 pageId
  return view;
}

/**
 * 根据 pageId 查找 HelpPage。
 *
 * @param pageId 页面 id
 * @returns 找到返回 HelpPage，找不到返回 undefined
 */
export function findHelpPage(pageId: string): HelpPage | undefined {
  return HELP_PAGES.find(p => p.pageId === pageId);
}

/**
 * Hook：根据当前视图派生默认选中的 pageId。
 *
 * @param activeView 当前视图
 * @param hasDetail 是否处于详情形态
 * @returns 默认选中的 pageId
 */
export function useDefaultPageId(activeView: View, hasDetail: boolean): string {
  return useMemo(() => {
    const pageId = viewToPageId(activeView, hasDetail);
    // 若该 pageId 已注册，直接用；否则回退到 '_overview'
    return findHelpPage(pageId) ? pageId : '_overview';
  }, [activeView, hasDetail]);
}
