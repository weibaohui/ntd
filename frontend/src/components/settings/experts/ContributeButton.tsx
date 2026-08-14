// 专家「分享到官方仓库」按钮：ShareToRepoButton 的专家适配层。
// 泛化后通用逻辑在 settings/contribute/ShareToRepoButton（PAT 引导 + ActionButton），
// 本组件只负责专家专属参数（actionType/提示词/占位符参数）与来源守卫。
// 独立「专家」页与「专家模板」Tab 复用，保证两条入口交互一致。

import type { ExpertMetadata } from '@/types/expert';
import {
  ShareToRepoButton,
  toHomePath,
} from '@/components/settings/contribute/ShareToRepoButton';
import { CONTRIBUTE_ACTION_TYPE, buildContributePrompt } from './contributePrompt';

// 专家版 ActionButton 参数键名沿用 {{expert_name}}/{{version}}/{{expert_dir}}，
// 与 experts/contributePrompt.ts 的占位符一一对应（026 用例锁定，不改动）。
/**
 * 分享按钮组件（专家版）。
 *
 * 来源守卫：分享只对用户自定义专家开放（source === 'user'）——
 * 系统/模板来源（从官方仓库同步到 ~/.ntd/bundled/experts/）的专家是只读资源，
 * 用户不能修改，也就不能把系统专家原样打包提 PR 回官方仓库，直接不渲染分享入口。
 * 双入口（专家详情 Modal + 专家模板 Tab 行操作）复用本组件，守卫同时覆盖两处。
 */
export function ContributeButton({
  expert,
  size = 'middle',
  iconOnly = false,
}: {
  expert: ExpertMetadata;
  size?: 'small' | 'middle';
  /** 仅图标模式（模板管理表格行内用）；详情 Modal 等场景保持文字 */
  iconOnly?: boolean;
}) {
  // 来源守卫：非用户来源直接不渲染（本封装无 Hooks，可安全提前 return）。
  if (expert.source !== 'user') return null;

  return (
    <ShareToRepoButton
      actionType={CONTRIBUTE_ACTION_TYPE}
      actionKey={expert.name}
      params={{
        expert_name: expert.name,
        version: expert.version,
        expert_dir: toHomePath(expert.definition_dir),
      }}
      buildPrompt={buildContributePrompt}
      panelTitle={`分享专家 ${expert.name}`}
      panelDescription="AI 将读取本机 PAT，把该专家打包为 PR 提交到官方仓库（可编辑下方 Prompt）"
      size={size}
      iconOnly={iconOnly}
    />
  );
}