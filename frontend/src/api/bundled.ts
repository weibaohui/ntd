// 内置资源同步 API
// 对应后端 /api/bundled/* 接口
// 统一管理专家、事项模板、Skills 的远程仓库同步

import { api, unwrap } from '@/utils/database/client';

/**
 * 子目录类型
 */
export type Subdir = 'all' | 'experts' | 'todos' | 'skills' | 'processes';

export interface BundledStatus {
  remote_url: string;
  branch: string;
  local_path: string;
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
 * 后端强制分页：page / page_size 始终有值，不会返回全量数据。
 * skills 是「当前页」的过滤后切片；total 是「过滤后」的计数，
 * 前端 Pagination 据此渲染页码。注意 total 与 skills.length 不一定相等。
 */
export interface BundledSkillsResponse {
  skills: BundledSkillMeta[];
  /** 来源分类信息（key 为 source 名称） */
  sources: Record<string, SkillSourceMeta>;
  /** 总数：「过滤后」的技能数（先按 source/keyword 过滤，再分页），前端据此渲染分页器 */
  total: number;
  /** 当前页码（从 1 开始） */
  page: number;
  /** 每页大小 */
  page_size: number;
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
 *
 * 注意：total 是「过滤后」的来源数（先按 keyword 过滤，再分页），
 * 而每个 source 内的 skill_count 仍是「过滤前」的真实技能数——
 * 两者语义故意不同：total 决定分页器，skill_count 展示「该来源下有多少技能」。
 */
export interface BundledSkillSourcesResponse {
  /** 当前页的来源列表（已分页切片） */
  sources: SkillSourceWithCount[];
  /** 来源总数：「过滤后」的来源数，前端 Pagination 据此渲染页码 */
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
 * 安装技能响应
 */
export interface InstallSkillResponse {
  success: boolean;
  message: string;
  target_path: string;
}

/**
 * 工艺模板列表项
 */
export interface ProcessTemplate {
  id: number;
  /** 040：全局唯一身份，寻址（详情/编辑/安装/复制）一律用 guid；name 只做展示，允许重复。 */
  guid: string;
  name: string;
  display_name: string;
  description: string;
  category: string;
  complexity: 'light' | 'standard' | 'complex';
  version: string;
  source_path: string | null;
  is_system: boolean;
  created_at: string | null;
  updated_at: string | null;
}

/**
 * 工艺模板详情
 */
export interface ProcessTemplateDetail extends ProcessTemplate {
  definition: string;
}

/**
 * 安装工艺模板响应
 */
export interface InstallProcessResponse {
  loop_id: number;
  loop_name: string;
  phase_count: number;
  step_count: number;
}

/**
 * 工艺实例环路列表项（工艺详情「实例环路」Tab 用）
 */
export interface ProcessLoopItem {
  id: number;
  name: string;
  description: string;
  status: string;
  workspace_id: number | null;
  /** 实例化时的工艺版本快照 */
  process_template_version: string | null;
  created_at: string | null;
  execution_count: number;
}

// ── M2 工艺运行时类型 ──────────────────────────────────

/** 产物快照 */
export interface ArtifactDto {
  id: number;
  name: string;
  artifact_type: 'file' | 'text' | 'url' | 'json';
  locator: string;
  content_text?: string;
  captured_at: string;
  captured_by?: string;
}

/** 门禁评价记录 */
export interface GateDto {
  id: number;
  gate_type: 'artifact_present' | 'ai_criteria_review' | 'human_approval' | 'script_check';
  gate_name: string;
  status: 'pending' | 'passed' | 'failed';
  result?: string;
  evaluated_at?: string;
  evaluated_by?: string;
}

/** 环节执行状态 */
export interface StepExecutionStatusDto {
  step_execution_id: number;
  sequence_index: number;
  status: string;
  rework_count: number;
  rating?: number;
  error_message?: string;
  conclusion?: string;
}

/** 环节审计 */
export interface StepAuditDto {
  step_id: number;
  step_name: string;
  order_index: number;
  skill_names: string[];
  execution?: StepExecutionStatusDto;
  artifacts: ArtifactDto[];
  gates: GateDto[];
}

/** 阶段执行状态 */
export interface PhaseExecutionStatusDto {
  status: string;
  started_at?: string;
  finished_at?: string;
}

/** 阶段审计 */
export interface PhaseAuditDto {
  phase_id: number;
  phase_name: string;
  execution: PhaseExecutionStatusDto;
  steps: StepAuditDto[];
}

/** 执行摘要 */
export interface LoopExecutionSummaryDto {
  id: number;
  loop_id: number;
  status: string;
  started_at: string;
  finished_at?: string;
  total_steps: number;
  completed_steps: number;
  failed_steps: number;
}

/** 工艺审计顶级结构 */
export interface ProcessAuditDto {
  loop_execution: LoopExecutionSummaryDto;
  phases: PhaseAuditDto[];
}

/**
 * 内置资源同步 API
 */
export const bundledApi = {
  /**
   * 手动触发同步
   */
  async sync(params: { subdir?: Subdir } = {}): Promise<SyncResult> {
    // 后端返回 {code, data, message} 包裹，必须用 unwrap 取出 data，
    // 否则调用方拿到的会是整个 axios response，字段访问全部失效。
    // 同步策略已固定为「以远程为准」，不再由前端传参。
    return unwrap(await api.post('/api/bundled/sync', {
      subdir: params.subdir || 'all',
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

  // ---------------------------------------------------------------------------
  // 工艺模板市场 API
  // ---------------------------------------------------------------------------

  /**
   * 获取工艺模板列表。
   *
   * 039：`isSystem` 有值时走服务端过滤（工艺列表页「我的/模板」双视图）；
   * 不传则返回全量——设置页模板管理等旧调用方依赖全量语义，不能默认过滤。
   */
  async getProcesses(isSystem?: boolean): Promise<ProcessTemplate[]> {
    const suffix = isSystem === undefined ? '' : `?is_system=${isSystem}`;
    return unwrap(await api.get(`/api/bundled/processes${suffix}`));
  },

  /**
   * 获取工艺模板详情（040：按 guid 寻址，同名模板不歧义）
   */
  async getProcess(guid: string): Promise<ProcessTemplateDetail> {
    return unwrap(await api.get(`/api/bundled/processes/${encodeURIComponent(guid)}`));
  },

  /**
   * 安装工艺模板到指定工作空间
   */
  async installProcess(guid: string, workspaceId: number): Promise<InstallProcessResponse> {
    return unwrap(await api.post(`/api/bundled/processes/${encodeURIComponent(guid)}/install`, {
      workspace_id: workspaceId,
    }));
  },

  /**
   * 列出该工艺模板实例化的环路（按创建时间倒序）。
   * 工艺详情「实例环路」Tab 用，支撑「工艺 → 环路」向下钻取。
   */
  async listProcessLoops(guid: string): Promise<ProcessLoopItem[]> {
    return unwrap(await api.get(`/api/v1/processes/${encodeURIComponent(guid)}/loops`));
  },

  /**
   * 升级工艺实例环路到模板最新版本（重新安装步骤/阶段）。
   */
  async upgradeProcessLoop(guid: string, loopId: number): Promise<InstallProcessResponse> {
    return unwrap(await api.post(`/api/v1/processes/${encodeURIComponent(guid)}/loops/${loopId}/upgrade`, {}));
  },

  /**
   * 复制工艺到用户层 ~/.ntd/processes/（040）。
   * 副本换新 guid 与源同名共存，原模板不消失；返回副本的 guid/name/路径。
   */
  async copyProcessToUser(guid: string): Promise<{ user_source_path: string; guid: string; name: string }> {
    return unwrap(await api.post(`/api/v1/processes/${encodeURIComponent(guid)}/copy-to-user`, {}));
  },

  /**
   * 保存工艺（M5）：PUT /api/v1/processes/{name}
   * yamlText 为编辑后的完整 YAML 文本，后端会做 serde_yaml 结构校验。
   * 系统工艺拒绝保存（409），前端 Toolbar 已禁用按钮，这里是兜底防线。
   *
   * body 为 JSON `{ definition: yamlText }`，对齐后端 `Json<UpdateProcessRequest>` extractor
   * （axum Json extractor 强制 Content-Type: application/json，发 text/yaml raw body 会被 415 拒）。
   */
  async putProcess(guid: string, yamlText: string): Promise<{ definition: string }> {
    // 后端 update_process 会自动递增版本号并回传含新版本的完整 YAML，
    // 前端需用它回刷 Monaco，避免陈旧 version 下次保存触发误判（需求 042）。
    return unwrap(await api.put(`/api/v1/processes/${encodeURIComponent(guid)}`, { definition: yamlText }));
  },

  /**
   * 新建工艺（M6）：POST /api/v1/processes
   * meta 含 name + 元信息 + definition（完整工艺 YAML 文本）。
   * 后端校验 name 唯一性（重名返回 409），前端 Modal 已做实时校验，这里是兜底。
   *
   * body 为 JSON 对齐后端 `Json<CreateProcessRequest>` extractor
   * （axum Json extractor 强制 Content-Type: application/json，发 text/yaml raw body 会被 415 拒）。
   */
  async postProcess(meta: {
    name: string;
    display_name?: string;
    description?: string;
    category?: string;
    complexity?: string;
    version?: string;
    definition: string;
  }): Promise<void> {
    // description 后端 CreateProcessRequest 无此字段（设计未要求），忽略不发
    await api.post('/api/v1/processes', meta);
  },

  /**
   * 删除工艺（M5）：DELETE /api/v1/processes/{name}
   * 系统工艺拒绝删除（409）；有实例 Loop 的工艺拒绝删除（409）。
   * 前端 Toolbar 仅在 !isSystem 时渲染删除按钮，这里是兜底防线。
   */
  async deleteProcess(guid: string): Promise<void> {
    await api.delete(`/api/v1/processes/${encodeURIComponent(guid)}`);
  },

  /**
   * 获取工艺实例审计数据（阶段 → 环节 → 产物 → 门禁）
   */
  async getProcessAudit(wsId: number, loopId: number, execId: number): Promise<ProcessAuditDto> {
    return unwrap(await api.get(`/api/v1/workspaces/${wsId}/loops/${loopId}/executions/${execId}/audit`));
  },

  /** 工艺仪表盘统计数据 */
  async getProcessStats(): Promise<{ template_stats: Array<{ name: string; display_name: string; complexity: string; loop_count: number }>; total_templates: number }> {
    return unwrap(await api.get('/api/v1/processes/stats'));
  },

  /** 工艺推荐 */
  async recommendProcesses(description: string): Promise<{ recommendations: Array<{ template_guid: string; template_name: string; display_name: string; complexity: string; score: number; reasons: string[] }> }> {
    return unwrap(await api.post('/api/v1/processes/recommend', { description }));
  },

  /** 创建任务：推荐→创建task→复用/创建Loop→创建执行 */
  async createTask(requirement: string, loopId: number, wsId: number): Promise<{ task_id: number; loop_id: number; execution_id: number }> {
    return unwrap(await api.post(`/api/v1/workspaces/${wsId}/tasks`, { requirement, loop_id: loopId }));
  },

  /** 为已有任务创建新执行 */
  async createTaskExecution(wsId: number, taskId: number, requirement: string): Promise<{ execution_id: number }> {
    return unwrap(await api.post(`/api/v1/workspaces/${wsId}/tasks/${taskId}/executions`, { requirement }));
  },

  /** 任务列表 */
  async listTasks(wsId: number, status?: string): Promise<Array<{ id: number; title: string; description: string; status: string; template_name?: string; complexity?: string; loop_id?: number; workspace_id?: number; latest_execution_status?: string; latest_execution_requirement?: string; created_at?: string }>> {
    const params = status ? { status } : {};
    return unwrap(await api.get(`/api/v1/workspaces/${wsId}/tasks`, { params }));
  },

  /** 任务详情 */
  async getTaskDetail(wsId: number, taskId: number): Promise<any> {
    return unwrap(await api.get(`/api/v1/workspaces/${wsId}/tasks/${taskId}`));
  },

  /**
   * 批量删除任务。
   */
  async batchDeleteTasks(wsId: number, ids: number[]): Promise<{ deleted: number; total: number }> {
    return unwrap(await api.post(`/api/v1/workspaces/${wsId}/tasks/batch-delete`, { ids }));
  },

  /**
   * 人工审批门禁
   */
  async approveGate(wsId: number, loopId: number, execId: number, stepExecId: number, gateId: number, approved: boolean, comment?: string): Promise<{ gate_id: number; status: string }> {
    return unwrap(await api.post(`/api/v1/workspaces/${wsId}/loops/${loopId}/executions/${execId}/steps/${stepExecId}/gates/${gateId}/approve`, {
      approved,
      comment,
    }));
  },

  /**
   * 手动补充产物
   */
  async addArtifact(wsId: number, loopId: number, execId: number, stepExecId: number, name: string, artifactType: string, locator: string, contentText?: string): Promise<ArtifactDto> {
    return unwrap(await api.post(`/api/v1/workspaces/${wsId}/loops/${loopId}/executions/${execId}/steps/${stepExecId}/artifacts`, {
      name,
      artifact_type: artifactType,
      locator,
      content_text: contentText,
    }));
  },
};

export default bundledApi;
