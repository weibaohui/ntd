// 评审模板 API 客户端。
//
// 后端路由在 backend/src/handlers/review_template.rs：
// - GET    /api/v1/review-templates          列表（含 prompt，管理面板用）
// - GET    /api/v1/review-templates/options  选项（不含 prompt，loop 选择器用）
// - GET    /api/v1/review-templates/{id}     详情
// - POST   /api/v1/review-templates          新建
// - PUT    /api/v1/review-templates/{id}     全量更新
// - DELETE /api/v1/review-templates/{id}     删除

import { api, unwrap } from './client';
import type {
  CreateReviewTemplateRequest,
  ReviewTemplate,
  ReviewTemplateOption,
  UpdateReviewTemplateRequest,
} from '@/types/reviewTemplate';

/** 列出全部评审模板（含 prompt）。管理面板用。可选按 workspace_id 过滤。 */
export async function listReviewTemplates(workspaceId?: number): Promise<ReviewTemplate[]> {
  const params = workspaceId !== undefined ? { workspace_id: workspaceId } : undefined;
  return unwrap(await api.get('/api/v1/review-templates', { params }));
}

/** 列出评审模板的轻量选项（不含 prompt）。loop 编辑器选择器用。可选按 workspace_id 过滤。 */
export async function listReviewTemplateOptions(workspaceId?: number): Promise<ReviewTemplateOption[]> {
  const params = workspaceId !== undefined ? { workspace_id: workspaceId } : undefined;
  return unwrap(await api.get('/api/v1/review-templates/options', { params }));
}

/** 创建模板。后端校验 name/prompt 非空。 */
export async function createReviewTemplate(req: CreateReviewTemplateRequest): Promise<ReviewTemplate> {
  return unwrap(await api.post('/api/v1/review-templates', req));
}

/** 全量更新模板（PUT 语义）。后端校验 name/prompt 非空。 */
export async function updateReviewTemplate(
  id: number,
  req: UpdateReviewTemplateRequest,
): Promise<ReviewTemplate> {
  return unwrap(await api.put(`/api/v1/review-templates/${id}`, req));
}

/** 删除模板。返回是否真的删了。 */
export async function deleteReviewTemplate(id: number): Promise<boolean> {
  return unwrap(await api.delete(`/api/v1/review-templates/${id}`));
}
