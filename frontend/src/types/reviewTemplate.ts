// 评审模板类型定义。
//
// 与 backend/src/models/mod.rs 的 ReviewTemplate / ReviewTemplateOption
// / CreateReviewTemplateRequest / UpdateReviewTemplateRequest 一一对应。
//
// 概念说明：V15 之后评审模板是独立表 review_templates（不再寄生在 todos
// 的 todo_type=1 行上）。模板本身只含 prompt，不带 executor——评审实例
// 运行时 executor 继承自被评审的源 todo / record。

/** 完整评审模板（管理面板用,含 prompt）。 */
export interface ReviewTemplate {
  id: number;
  name: string;
  description: string | null;
  prompt: string;
  /** 所属工作空间（目录路径）。null=全局模板。 */
  workspace: string | null;
  created_at: string | null;
  updated_at: string | null;
}

/** 评审模板轻量选项（loop 选择器用,不含 prompt）。 */
export interface ReviewTemplateOption {
  id: number;
  name: string;
  description: string | null;
  /** 所属工作空间（目录路径）。null=全局模板。 */
  workspace: string | null;
}

/** 创建评审模板请求体。 */
export interface CreateReviewTemplateRequest {
  name: string;
  description?: string | null;
  prompt: string;
  /** 所属工作空间（目录路径）。不传=全局模板。 */
  workspace?: string | null;
}

/** 全量更新评审模板请求体（PUT 语义）。 */
export interface UpdateReviewTemplateRequest {
  name: string;
  description?: string | null;
  prompt: string;
}
