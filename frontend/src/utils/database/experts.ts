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
