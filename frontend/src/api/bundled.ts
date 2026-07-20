// 内置资源同步 API
// 对应后端 /api/bundled/* 接口
// 统一管理专家、事项模板、Skills 的远程仓库同步

import { api, unwrap } from '@/utils/database/client';

/**
 * 同步策略
 */
export type SyncStrategy = 'keep_local' | 'overwrite' | 'manual';

/**
 * 子目录类型
 */
export type Subdir = 'all' | 'experts' | 'todos' | 'skills';

export interface BundledStatus {
  remote_url: string;
  branch: string;
  local_path: string;
  sync_strategy: string;
  auto_sync_enabled: boolean;
  local_exists: boolean;
  local_commit: string | null;
  remote_commit: string | null;
  needs_update: boolean | null;
  last_sync_at: string | null;
  subdir: string;
  subdir_exists: boolean;
  subdir_file_count: number;
  /** 环境中是否安装了 git（同步的前置依赖）；false 时前端展示「一键安装 Git」入口 */
  git_available: boolean;
}

export interface BundledConfig {
  url: string;
  branch: string;
  local_path: string;
  auto_sync_enabled: boolean;
  auto_sync_cron: string;
  last_sync_at: string | null;
}

export interface SyncResult {
  success: boolean;
  message: string;
  is_first_clone: boolean;
  has_updates: boolean;
  changed_files: number;
  subdir: string;
}

/**
 * 技能来源元数据
 * 从 skills/{source}/metadata.json 读取的信息
 */
export interface SkillSourceMeta {
  /** 来源标识（与目录名一致） */
  name: string;
  /** 展示名称 */
  display_name: string;
  /** 来源描述 */
  description: string;
  /** GitHub 地址 */
  github_url: string;
  /** Star 数量 */
  stars: number;
  /** 许可证 */
  license?: string;
  /** 作者/组织 */
  author?: string;
}

/**
 * Bundled Skill 元数据
 * 从 ~/.ntd/bundled/skills/ 目录扫描得到的技能信息
 */
export interface BundledSkillMeta {
  /** 完整路径名（如 awesome-skills-zh/lark-doc） */
  name: string;
  /** 短名称（最后一段，如 lark-doc） */
  short_name: string;
  /** 来源（第一段目录名，如 awesome-skills-zh） */
  source: string;
  /** 来源元数据 */
  source_meta?: SkillSourceMeta;
  /** 描述 */
  description: string;
  /** 中文描述 */
  description_zh?: string;
  /** 版本号 */
  version?: string;
  /** 作者 */
  author?: string;
  /** 许可证 */
  license?: string;
  /** 文件数 */
  file_count: number;
  /** 总大小（字节） */
  total_size: number;
  /** 最后修改时间 */
  modified_at?: string;
}

/**
 * Bundled Skills 列表响应
 *
 * 后端支持 `?page=&page_size=` 分页；不带分页参数时仍返回全量，保持向后兼容。
 * 分页字段只在请求带了 page / page_size 时才会有值。
 */
export interface BundledSkillsResponse {
  skills: BundledSkillMeta[];
  /** 来源分类信息（key 为 source 名称） */
  sources: Record<string, SkillSourceMeta>;
  /** 总数：分页时是「过滤前」的全量技能数，前端据此渲染分页器 */
  total: number;
  /** 当前页码（从 1 开始）；不分页时为 undefined */
  page?: number;
  /** 每页大小；不分页时为 undefined */
  page_size?: number;
}

/**
 * 带技能计数的来源视图
 *
 * 来源分页接口专用：在 SkillSourceMeta 基础上附加 `skill_count`，
 * 让前端来源网格能直接显示「该来源下有多少技能」。
 */
export interface SkillSourceWithCount {
  /** 来源元数据 */
  meta: SkillSourceMeta;
  /** 该来源下的技能数（过滤前计数） */
  skill_count: number;
}

/**
 * 来源分页列表响应
 *
 * 与 BundledSkillsResponse 职责分离：
 * - BundledSkillsResponse 按「技能」切片，用于「全部技能」模式
 * - BundledSkillSourcesResponse 按「来源」切片，用于「按来源浏览」来源网格
 */
export interface BundledSkillSourcesResponse {
  /** 当前页的来源列表（已分页切片） */
  sources: SkillSourceWithCount[];
  /** 来源总数（过滤前），前端 Pagination 据此渲染页码 */
  total: number;
  /** 当前页码（从 1 开始） */
  page: number;
  /** 每页大小 */
  page_size: number;
}

/**
 * Bundled Skill 文件信息
 */
export interface BundledSkillFile {
  /** 相对路径 */
  path: string;
  /** 文件大小（字节） */
  size: number;
}

/**
 * Bundled Skill 内容响应
 */
export interface BundledSkillContentResponse {
  /** 技能名称 */
  skill_name: string;
  /** SKILL.md 文本内容 */
  content: string;
  /** 文件列表 */
  files: BundledSkillFile[];
}

/**
 * 安装技能请求
 */
export interface InstallSkillRequest {
  /** 技能完整路径名 */
  skill_name: string;
  /** 目标执行器 */
  executor: string;
}

/**
 * 安装技能响应
 */
export interface InstallSkillResponse {
  success: boolean;
  message: string;
  target_path: string;
}

/**
 * 内置资源同步 API
 */
export const bundledApi = {
  /**
   * 手动触发同步
   */
  async sync(params: { subdir?: Subdir; strategy?: SyncStrategy } = {}): Promise<SyncResult> {
    // 后端返回 {code, data, message} 包裹，必须用 unwrap 取出 data，
    // 否则调用方拿到的会是整个 axios response，字段访问全部失效。
    return unwrap(await api.post('/api/bundled/sync', {
      subdir: params.subdir || 'all',
      strategy: params.strategy || 'keep_local',
    }));
  },

  /**
   * 查询同步状态
   */
  async getStatus(subdir: Subdir = 'all'): Promise<BundledStatus> {
    return unwrap(await api.get('/api/bundled/status', { params: { subdir } }));
  },

  /**
   * 获取配置
   */
  async getConfig(): Promise<BundledConfig> {
    return unwrap(await api.get('/api/bundled/config'));
  },

  /**
   * 更新配置
   */
  async updateConfig(config: Partial<BundledConfig>): Promise<BundledConfig> {
    return unwrap(await api.put('/api/bundled/config', config));
  },

  // ---------------------------------------------------------------------------
  // 技能市场 API
  // ---------------------------------------------------------------------------

  /**
   * 获取技能市场中的所有技能
   * 扫描 ~/.ntd/bundled/skills/ 目录，返回可安装的技能列表
   *
   * 强制分页：page / page_size 必传，后端只返回该页切片。
   * 过滤参数 source / keyword 下沉到后端：先按它们过滤，再分页，
   * 这样 total 就是「过滤后」的计数，前端 Pagination 与实际可见技能一一对应。
   * 绝不会返回全量数据。
   */
  async getSkills(params: {
    page: number;
    page_size: number;
    /** 来源筛选：传具体 source 名只返回该来源的技能 */
    source?: string;
    /** 关键字筛选：不区分大小写匹配 name / short_name / description / description_zh */
    keyword?: string;
  }): Promise<BundledSkillsResponse> {
    // axios 会自动忽略 undefined 字段，所以前端只下发「显式传了」的过滤参数
    return unwrap(await api.get('/api/bundled/skills', { params }));
  },

  /**
   * 获取技能来源分页列表
   *
   * 与 getSkills 职责分离：
   * - getSkills 按「技能」切片，用于「全部技能」模式
   * - getSkillSources 按「来源」切片，用于「按来源浏览」来源网格
   *
   * 每个来源附 skill_count（过滤前计数），前端来源卡片据此显示数量。
   */
  async getSkillSources(params: {
    page: number;
    page_size: number;
    /** 来源关键字筛选：不区分大小写匹配 name / display_name / description */
    keyword?: string;
  }): Promise<BundledSkillSourcesResponse> {
    return unwrap(await api.get('/api/bundled/skill-sources', { params }));
  },

  /**
   * 获取技能的 SKILL.md 内容和文件列表
   * 用于详情 Drawer 展示
   */
  async getSkillContent(skillName: string): Promise<BundledSkillContentResponse> {
    return unwrap(await api.get(`/api/bundled/skills/${encodeURIComponent(skillName)}/content`));
  },

  /**
   * 读取 bundled 技能内单个文件的内容
   * 用于市场页文件浏览器预览 SKILL.md 以外的文件
   */
  async getSkillFileContent(skillName: string, path: string): Promise<{ path: string; content: string }> {
    // path 作为 query 参数透传，axios 会自动 encode；skillName 含 `/` 需手动 encode 进路径段
    return unwrap(await api.get(`/api/bundled/skills/${encodeURIComponent(skillName)}/file`, {
      params: { path },
    }));
  },

  /**
   * 安装技能到指定执行器
   * 将 bundled/skills/{skill_name} 复制到目标执行器的 skills 目录
   */
  async installSkill(skillName: string, executor: string): Promise<InstallSkillResponse> {
    return unwrap(await api.post('/api/bundled/skills/install', {
      skill_name: skillName,
      executor,
    }));
  },
};

export default bundledApi;
