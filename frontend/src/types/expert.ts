// ─── 专家系统类型定义 ────────────────────────────────────────
//
// 兼容 WorkBuddy 的 plugin.json 格式，支持单个专家和专家团队两种类型。
// 数据来源：后端 /api/experts 接口，从 ~/.ntd/experts/ 目录加载。

/** 专家类型：单个专家 或 专家团队 */
export type ExpertType = 'agent' | 'team';

/** 专家来源：系统内置 或 用户自定义 */
export type ExpertSource = 'system' | 'user';

/** 成员角色：负责人 或 成员 */
export type MemberRole = 'lead' | 'member';

/** 专家标签（多语言） */
export interface ExpertTag {
  zh: string;
  en: string;
}

/** 专家成员（团队类型使用） */
export interface ExpertMember {
  id: string;
  name_zh?: string;
  name_en?: string;
  profession_zh?: string;
  profession_en?: string;
  avatar_path?: string;
  role: MemberRole;
}

/** 技能元数据（从 SKILL.md 解析） */
export interface SkillMetadata {
  skill_name: string;
  skill_dir: string;
  skill_md_path: string;
  yaml_name?: string;
  yaml_description?: string;
  yaml_description_zh?: string;
  yaml_description_en?: string;
  yaml_version?: string;
  yaml_allowed_tools: string[];
  yaml_emoji?: string;
}

/** 专家元数据（内存索引，从 plugin.json 解析） */
export interface ExpertMetadata {
  /** 专家 ID（全局唯一） */
  name: string;
  /** 类型：单个专家 或 专家团队 */
  expert_type: ExpertType;
  /** 版本号 */
  version: string;

  /** 中文名 */
  display_name_zh?: string;
  /** 英文名 */
  display_name_en?: string;
  /** 中文职业 */
  profession_zh?: string;
  /** 英文职业 */
  profession_en?: string;
  /** 中文描述 */
  description_zh?: string;
  /** 英文描述 */
  description_en?: string;

  /** 头像相对路径 */
  avatar_path?: string;
  /** 分类 ID */
  category_id?: string;

  /** 定义文件所在目录（绝对路径） */
  definition_dir: string;
  /** plugin.json 绝对路径 */
  plugin_json_path: string;

  /** 单个专家的 agent_name */
  agent_name?: string;

  /** 团队负责人 ID（仅 team 类型） */
  lead_agent?: string;
  /** 团队成员 ID 列表（仅 team 类型） */
  member_agents: string[];
  /** 团队成员详情（仅 team 类型） */
  members: ExpertMember[];

  /** 技能路径列表（相对路径） */
  skills: string[];

  /** 默认初始提示词（中文） */
  default_init_prompt_zh?: string;
  /** 默认初始提示词（英文） */
  default_init_prompt_en?: string;

  /** 标签列表 */
  tags: ExpertTag[];

  /** 加载时间 */
  loaded_at: string;
  /** 是否激活 */
  is_active: boolean;
  /** 专家来源（系统内置 / 用户自定义） */
  source: ExpertSource;
}

/** 加载结果 */
export interface LoadResult {
  loaded_count: number;
  errors: string[];
}

/** 获取展示名称（中文优先，回退英文，最后回退 name） */
export function getExpertDisplayName(expert: ExpertMetadata): string {
  return expert.display_name_zh || expert.display_name_en || expert.name;
}

/** 获取展示描述（中文优先，回退英文） */
export function getExpertDescription(expert: ExpertMetadata): string {
  return expert.description_zh || expert.description_en || '';
}

/** 获取职业（中文优先，回退英文） */
export function getExpertProfession(expert: ExpertMetadata): string {
  return expert.profession_zh || expert.profession_en || '';
}

/** 获取头像 URL */
export function getExpertAvatarUrl(expert: ExpertMetadata): string {
  if (!expert.avatar_path) return '';
  return `/api/experts/${encodeURIComponent(expert.name)}/avatar`;
}

/**
 * 获取团队成员头像 URL
 *
 * 成员的 avatar_path 是相对路径（如 avatars/xxx.jpg），浏览器无法直接访问，
 * 需要通过后端接口 /api/experts/:name/members/:member_id/avatar 获取。
 * 后端会根据 expert_name 定位专家目录，再在 members 中按 member_id 查找成员，
 * 拼接 definition_dir + member.avatar_path 读取头像文件。
 */
export function getMemberAvatarUrl(expertName: string, memberId: string): string {
  return `/api/experts/${encodeURIComponent(expertName)}/members/${encodeURIComponent(memberId)}/avatar`;
}

/**
 * 获取分类名称映射。
 *
 * 根据 plugin.json 中的 categoryId 字段返回中文分类名称。
 * 分类 ID 格式如 "02-Engineering"、"08-FinanceInvestment"；未命中映射时回退到
 * 去掉前缀编号后的原始片段，保证未知分类也有可读文本而非裸 ID。
 */
export function getCategoryName(categoryId?: string): string {
  if (!categoryId) return '';
  const map: Record<string, string> = {
    '02-Engineering': '工程技术',
    '08-FinanceInvestment': '金融投资',
  };
  return map[categoryId] || categoryId.split('-').slice(1).join(' ');
}
