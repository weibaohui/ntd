// 评审模板 API 客户端。
//
// 后端路由在 backend/src/handlers/review_template.rs：
// - GET    /api/review-templates          列表（含 prompt，管理面板用）
// - GET    /api/review-templates/options  选项（不含 prompt，loop 选择器用）
// - GET    /api/review-templates/{id}     详情
// - POST   /api/review-templates          新建
// - PUT    /api/review-templates/{id}     全量更新
// - DELETE /api/review-templates/{id}     删除

import { api, unwrap } from './client';
import type {
  CreateReviewTemplateRequest,
  ReviewTemplate,
  ReviewTemplateOption,
  UpdateReviewTemplateRequest,
} from '@/types/reviewTemplate';

/** 列出全部评审模板（含 prompt）。管理面板用。可选按 workspace 过滤。 */
export async function listReviewTemplates(workspace?: string): Promise<ReviewTemplate[]> {
  const params = workspace ? { workspace } : undefined;
  return unwrap(await api.get('/api/review-templates', { params }));
}

/** 列出评审模板的轻量选项（不含 prompt）。loop 编辑器选择器用。可选按 workspace 过滤。 */
export async function listReviewTemplateOptions(workspace?: string): Promise<ReviewTemplateOption[]> {
  const params = workspace ? { workspace } : undefined;
  return unwrap(await api.get('/api/review-templates/options', { params }));
}

/** 取单条模板。 */
export async function getReviewTemplate(id: number): Promise<ReviewTemplate> {
  return unwrap(await api.get(`/api/review-templates/${id}`));
}

/** 创建模板。后端校验 name/prompt 非空。 */
export async function createReviewTemplate(req: CreateReviewTemplateRequest): Promise<ReviewTemplate> {
  return unwrap(await api.post('/api/review-templates', req));
}

/** 全量更新模板（PUT 语义）。后端校验 name/prompt 非空。 */
export async function updateReviewTemplate(
  id: number,
  req: UpdateReviewTemplateRequest,
): Promise<ReviewTemplate> {
  return unwrap(await api.put(`/api/review-templates/${id}`, req));
}

/** 删除模板。返回是否真的删了。 */
export async function deleteReviewTemplate(id: number): Promise<boolean> {
  return unwrap(await api.delete(`/api/review-templates/${id}`));
}
