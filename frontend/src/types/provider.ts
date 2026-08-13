// Provider（AI 服务商 / API Key 管理）领域类型。
//
// 集中定义 ProfilesPanel 与 utils/database/providers Facade 共享的类型，
// 对齐后端 handlers/profiles.rs 的响应结构（list 回 summary、单查回 detail）。

/** 单个模型定义。display_name / supports_1m_context 可选。 */
export interface ProviderModel {
  name: string;
  display_name?: string;
  supports_1m_context?: boolean;
}

/** 列表接口返回的摘要——不含 api_key / models，只给计数。对应后端 ProviderSummary。 */
export interface ProviderSummary {
  name: string;
  display_name: string;
  base_url: string;
  protocol: 'openai' | 'anthropic';
  model_count: number;
}

/** 单查接口返回的完整详情，含明文 api_key（组件用 maskKey 打码展示）。对应后端 ProviderDetail。 */
export interface ProviderDetail {
  name: string;
  display_name: string;
  api_key: string;
  base_url: string;
  protocol: 'openai' | 'anthropic';
  models: ProviderModel[];
}

/** 执行器配置定义——/supported-executors 返回，标识每个执行器的配置文件路径与是否有生成器。 */
export interface ExecutorConfigDef {
  name: string;
  display_name: string;
  config_path: string;
  has_generator: boolean;
}

/** create/update 请求体。name 仅 create 用（update 走 URL 路径参数），故可选。 */
export interface ProviderInput {
  name?: string;
  display_name: string;
  api_key: string;
  base_url: string;
  protocol: 'openai' | 'anthropic';
  models: ProviderModel[];
}

/** apply 预览的单条结果：某个执行器将被写入的文件路径与内容。 */
export interface PreviewEntry {
  executor: string;
  model: string;
  path: string;
  content: string;
}

/** apply 执行结果：成功的执行器名 + 失败原因列表。 */
export interface ApplyResult {
  applied: string[];
  errors: string[];
}

/** import 结果：成功导入的名称 + 错误列表。 */
export interface ImportResult {
  imported: string[];
  errors: string[];
}
