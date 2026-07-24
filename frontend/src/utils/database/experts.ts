import { api, unwrap } from './client';
import type { ExpertMetadata, SkillMetadata, LoadResult } from '@/types/expert';

// 模块级专家元数据缓存：专家数据几乎不变（仅在导入/删除/重载时变化），
// 列表中大量 ExpertBadge 共享同一专家时复用缓存，避免 N 次重复请求。
// inflightMap 合并同名并发请求，防止首屏瞬时并发雪崩。
const expertCache = new Map<string, ExpertMetadata>();
const inflightMap = new Map<string, Promise<ExpertMetadata>>();

/**
 * 获取所有专家列表
 *
 * 从后端内存索引中获取已加载的专家元数据，包括单个专家和专家团队。
 */
export async function getAllExperts(): Promise<ExpertMetadata[]> {
  return unwrap(await api.get('/api/v1/experts'));
}

/**
 * 获取单个专家详情
 *
 * 根据专家名称获取完整的元数据信息。
 */
export async function getExpertByName(name: string): Promise<ExpertMetadata> {
  return unwrap(await api.get(`/api/v1/experts/${encodeURIComponent(name)}`));
}

/**
 * 获取单个专家详情（带内存缓存 + 请求合并）。
 *
 * 列表中大量 ExpertBadge 会按名重复拉取同一专家，直接走 getExpertByName 会产生 N 次请求。
 * 这里：命中缓存直接返回；已有 in-flight 请求则复用同一 Promise；否则发请求，成功后写缓存。
 * 失败不写缓存，让下次重试。专家增删改后由 invalidateExpertCache 失效。
 */
export async function getExpertByNameCached(name: string): Promise<ExpertMetadata> {
  const cached = expertCache.get(name);
  if (cached) return cached;
  const inflight = inflightMap.get(name);
  if (inflight) return inflight;
  const promise = getExpertByName(name)
    .then((meta) => {
      expertCache.set(name, meta);
      inflightMap.delete(name);
      return meta;
    })
    .catch((err) => {
      // 失败时移除 in-flight 标记，让下次调用可以重试，不缓存错误结果。
      inflightMap.delete(name);
      throw err;
    });
  inflightMap.set(name, promise);
  return promise;
}

/**
 * 失效专家缓存。
 *
 * 专家删除/导入/重载后调用，保证 ExpertBadge 不展示陈旧数据。
 * 传 name 只清单个专家；不传则全量清空（用于 reload / 批量导入等可能影响多个专家的场景）。
 */
export function invalidateExpertCache(name?: string): void {
  if (name) {
    expertCache.delete(name);
    inflightMap.delete(name);
  } else {
    expertCache.clear();
    inflightMap.clear();
  }
}

/**
 * 获取专家的 Agent MD 内容
 *
 * 根据专家类型自动定位：
 * - 单个专家：使用 agent_name 字段
 * - 专家团队：使用 lead_agent 字段
 *
 * 返回完整的 MD 文件内容，用于执行时注入 prompt。
 */
export async function getExpertAgentMd(name: string): Promise<string> {
  return unwrap(await api.get(`/api/v1/experts/${encodeURIComponent(name)}/agent-md`));
}

/**
 * 获取专家关联的所有 Skill 元数据
 *
 * 返回专家绑定的技能列表，用于前端展示可用技能。
 */
export async function getExpertSkills(name: string): Promise<SkillMetadata[]> {
  return unwrap(await api.get(`/api/v1/experts/${encodeURIComponent(name)}/skills`));
}

/**
 * 重新加载所有专家定义
 *
 * 清空现有索引，重新扫描 ~/.ntd/experts/ 目录加载专家定义。
 * 返回加载结果（成功数量和错误列表）。
 */
export async function reloadExperts(): Promise<LoadResult> {
  return unwrap(await api.post('/api/v1/experts/reload'));
}

/**
 * 删除专家
 *
 * 删除指定专家及其磁盘上的定义目录。删除后该专家不再出现在列表中，
 * 也无法被选择使用。此操作不可逆，请谨慎操作。
 */
export async function deleteExpert(name: string): Promise<void> {
  await api.delete(`/api/v1/experts/${encodeURIComponent(name)}`);
}

/**
 * 通过 AI 创建专家
 *
 * 根据 AI 生成的 plugin_json 和 agent_md 内容创建新专家。
 * 后端会自动创建目录结构并加载到索引中。
 */
export async function createExpertFromAi(pluginJson: string, agentMd: string): Promise<void> {
  await api.post('/api/v1/experts/create', {
    plugin_json: pluginJson,
    agent_md: agentMd,
  });
}

/**
 * 获取专家的原始 plugin.json 内容
 *
 * 返回 plugin.json 文件的原始文本内容，用于前端编辑。
 */
export async function getExpertPluginJson(name: string): Promise<string> {
  return unwrap(await api.get(`/api/v1/experts/${encodeURIComponent(name)}/plugin-json`));
}

/**
 * 更新专家
 *
 * 更新指定专家的 plugin.json 和 agent.md 内容。
 * 专家名称不可修改，如需改名请删除后重新创建。
 */
export async function updateExpert(name: string, pluginJson: string, agentMd: string): Promise<void> {
  await api.put(`/api/v1/experts/${encodeURIComponent(name)}`, {
    plugin_json: pluginJson,
    agent_md: agentMd,
  });
}

/**
 * 导出专家为 zip 文件
 *
 * 将指定专家的整个目录打包为 zip 文件下载。
 * 返回 Blob 类型的二进制数据，前端通过 a 标签触发下载。
 */
export async function exportExpert(name: string): Promise<Blob> {
  const response = await api.get(`/api/v1/experts/${encodeURIComponent(name)}/export`, {
    responseType: 'blob',
  });
  return response.data;
}

/**
 * 导入专家 zip 包
 *
 * 接收 multipart/form-data 上传的 zip 文件，解压并导入到 ~/.ntd/experts/ 目录。
 * 返回导入结果（成功的专家信息 + 错误列表）。
 */
export async function importExpert(file: File): Promise<{ expert: ExpertMetadata | null; errors: string[] }> {
  const formData = new FormData();
  formData.append('file', file);
  // 不手动设 Content-Type：交给 axios/浏览器自动生成带 boundary 的 multipart 头，
  // 手动设 multipart/form-data 会缺少 boundary 导致后端无法解析请求体。
  return unwrap(await api.post('/api/v1/experts/import', formData));
}

/**
 * 从本地目录导入专家
 *
 * 指定一个本地目录路径，将其复制到 ~/.ntd/experts/ 目录。
 * 用于从 WorkBuddy 插件目录批量导入专家。
 */
export async function importExpertFromDirectory(path: string): Promise<{ expert: ExpertMetadata | null; errors: string[] }> {
  return unwrap(await api.post('/api/v1/experts/import-from-directory', { path }));
}

/**
 * 从 WorkBuddy 批量导入专家
 *
 * 扫描 ~/.workbuddy/plugins/marketplaces/experts/plugins/ 目录，
 * 将所有未导入的专家/专家团队批量复制到 ~/.ntd/experts/ 目录。
 * 已存在的专家会被跳过，不会覆盖。
 */
export async function importFromWorkbuddy(): Promise<WorkbuddyImportResult> {
  return unwrap(await api.post('/api/v1/experts/import-from-workbuddy'));
}

/** 从 WorkBuddy 批量导入的结果 */
export interface WorkbuddyImportResult {
  /** 成功导入的专家数量 */
  imported_count: number;
  /** 跳过的专家（已存在）数量 */
  skipped_count: number;
  /** 成功导入的专家名称列表 */
  imported: string[];
  /** 跳过的专家名称列表 */
  skipped: string[];
  /** 错误列表 */
  errors: string[];
}
