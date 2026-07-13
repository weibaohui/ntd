import { api, unwrap } from './client';
import type { ExpertMetadata, SkillMetadata, LoadResult } from '@/types/expert';

/**
 * 获取所有专家列表
 *
 * 从后端内存索引中获取已加载的专家元数据，包括单个专家和专家团队。
 */
export async function getAllExperts(): Promise<ExpertMetadata[]> {
  return unwrap(await api.get('/api/experts'));
}

/**
 * 获取单个专家详情
 *
 * 根据专家名称获取完整的元数据信息。
 */
export async function getExpertByName(name: string): Promise<ExpertMetadata> {
  return unwrap(await api.get(`/api/experts/${encodeURIComponent(name)}`));
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
  return unwrap(await api.get(`/api/experts/${encodeURIComponent(name)}/agent-md`));
}

/**
 * 获取专家关联的所有 Skill 元数据
 *
 * 返回专家绑定的技能列表，用于前端展示可用技能。
 */
export async function getExpertSkills(name: string): Promise<SkillMetadata[]> {
  return unwrap(await api.get(`/api/experts/${encodeURIComponent(name)}/skills`));
}

/**
 * 重新加载所有专家定义
 *
 * 清空现有索引，重新扫描 ~/.ntd/experts/ 目录加载专家定义。
 * 返回加载结果（成功数量和错误列表）。
 */
export async function reloadExperts(): Promise<LoadResult> {
  return unwrap(await api.post('/api/experts/reload'));
}

/**
 * 导出专家为 zip 文件
 *
 * 将指定专家的整个目录打包为 zip 文件下载。
 * 返回 Blob 类型的二进制数据，前端通过 a 标签触发下载。
 */
export async function exportExpert(name: string): Promise<Blob> {
  const response = await api.get(`/api/experts/${encodeURIComponent(name)}/export`, {
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
export async function importExpert(file: File): Promise<{ expert: ExpertMetadata; errors: string[] }> {
  const formData = new FormData();
  formData.append('file', file);
  return unwrap(await api.post('/api/experts/import', formData, {
    headers: { 'Content-Type': 'multipart/form-data' },
  }));
}

/**
 * 从本地目录导入专家
 *
 * 指定一个本地目录路径，将其复制到 ~/.ntd/experts/ 目录。
 * 用于从 WorkBuddy 插件目录批量导入专家。
 */
export async function importExpertFromDirectory(path: string): Promise<{ expert: ExpertMetadata; errors: string[] }> {
  return unwrap(await api.post('/api/experts/import-from-directory', { path }));
}

/**
 * 从 WorkBuddy 批量导入专家
 *
 * 扫描 ~/.workbuddy/plugins/marketplaces/experts/plugins/ 目录，
 * 将所有未导入的专家/专家团队批量复制到 ~/.ntd/experts/ 目录。
 * 已存在的专家会被跳过，不会覆盖。
 */
export async function importFromWorkbuddy(): Promise<WorkbuddyImportResult> {
  return unwrap(await api.post('/api/experts/import-from-workbuddy'));
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
