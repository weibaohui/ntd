// 工艺模板 YAML 定义的 TypeScript 镜像类型。
// 与 backend `services/process/mod.rs` 的 ProcessDefinition / LinkDefinition 等一一对应，
// 通过 js-yaml 解析 `ProcessTemplateDetail.definition` 得到。

/** 期望产物 */
export interface ExpectedArtifact {
  name: string;
  type: string;
  path?: string;
  locator?: string;
}

/** spec 模板文件引用（name + path），执行时注入 AI 上下文供其重点阅读 */
export interface StepTemplateRef {
  name: string;
  path: string;
}

/** 门禁 */
export interface GateDefinition {
  name: string;
  type: string;
  artifact?: string;
  criteria_ref?: string;
  min_score?: number;
  script?: string;
  /** AI 评审等待评分的超时秒数。null/0=不等待直接判定失败；正数=最多等 N 秒出分后判定。 */
  timeout_secs?: number;
}

/** 环节（Link）定义 */
export interface LinkDefinition {
  id: string;
  name: string;
  step_template?: StepTemplateRef[];
  /** 环节级验收标准（内联，原由原型表提供，现随 step_template 解耦） */
  acceptance_criteria?: string;
  prompt?: string;
  executor?: string;
  expert?: string;
  skills?: string[];
  model?: string;
  review_type?: string;
  expected_artifacts?: ExpectedArtifact[];
  gates?: GateDefinition[];
  on_success?: string;
  on_gate_fail?: string;
  max_rework?: number;
}

/** 阶段定义 */
export interface PhaseDefinition {
  id: string;
  name: string;
  spec?: string;
  acceptance_criteria?: string;
  links?: LinkDefinition[];
}

/** 工艺元信息 */
export interface ProcessMeta {
  name: string;
  display_name?: string;
  description?: string;
  category?: string;
  complexity?: string;
  version?: string;
}

/** 全局限制 */
export interface ProcessLimits {
  max_step_executions?: number;
  max_total_tokens?: number;
}

/** 工艺完整定义（YAML 顶层） */
export interface ProcessDefinition {
  process: ProcessMeta;
  limits?: ProcessLimits;
  phases?: PhaseDefinition[];
  step_templates?: unknown[];
  abnormal_handler?: unknown;
}
