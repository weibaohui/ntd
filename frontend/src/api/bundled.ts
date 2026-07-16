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
 */
export interface BundledSkillsResponse {
  skills: BundledSkillMeta[];
  /** 来源分类信息（key 为 source 名称） */
  sources: Record<string, SkillSourceMeta>;
  total: number;
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
   */
  async getSkills(): Promise<BundledSkillsResponse> {
    return unwrap(await api.get('/api/bundled/skills'));
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
