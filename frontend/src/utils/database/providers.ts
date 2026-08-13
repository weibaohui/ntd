// Provider（AI 服务商 / API Key）数据访问 Facade。
//
// 收口 ProfilesPanel 原先散落的 11 处裸 fetch：统一走 client.ts 的 axios 实例，
// 自动获得 v1 前缀处理、GET 网络错重试、{code,data,message} 解包与统一错误信息。
// 仿 experts.ts 的写法：除文本端点外一律 `unwrap(await api.<method>(...))`。

import { api, unwrap } from './client';
import type {
  ProviderSummary,
  ProviderDetail,
  ProviderInput,
  ExecutorConfigDef,
  PreviewEntry,
  ApplyResult,
  ImportResult,
} from '@/types/provider';

// 后端 providers 路由挂在 /api/v1/ 下（已是 v1，client.ts 的 v1 重写拦截器会跳过，不双前缀）。
const BASE = '/api/v1/providers';

/** 执行器→模型名 的映射，apply/preview 两个端点共用同一请求体形状。 */
type ExecutorModels = Record<string, string>;

/**
 * 列出所有 provider 摘要。
 *
 * 后端只回 summary（不含 api_key / models），需要完整详情须再逐个 getProvider。
 * ProfilesPanel 的加载流程保留这个 list→detail 的 N+1（组件需要 api_key 做打码、models 做展示）。
 */
export async function listProviders(): Promise<ProviderSummary[]> {
  return unwrap<ProviderSummary[]>(await api.get(BASE));
}

/** 获取所有执行器配置定义（配置文件路径、是否有生成器），用于 apply 弹窗的可选执行器列表。 */
export async function getSupportedExecutors(): Promise<ExecutorConfigDef[]> {
  return unwrap<ExecutorConfigDef[]>(await api.get(`${BASE}/supported-executors`));
}

/** 按 name 取单个 provider 完整详情。encodeURIComponent 防止标识符里的特殊字符破坏路径。 */
export async function getProvider(name: string): Promise<ProviderDetail> {
  return unwrap<ProviderDetail>(await api.get(`${BASE}/${encodeURIComponent(name)}`));
}

/** 新建 provider。后端返回 summary 但组件不需要，故丢弃响应只看是否成功。 */
export async function createProvider(body: ProviderInput): Promise<void> {
  await api.post(BASE, body);
}

/** 更新 provider。name 走 URL 路径参数，body 里其余字段覆盖写回。 */
export async function updateProvider(name: string, body: ProviderInput): Promise<void> {
  await api.put(`${BASE}/${encodeURIComponent(name)}`, body);
}

/** 删除 provider。 */
export async function deleteProvider(name: string): Promise<void> {
  await api.delete(`${BASE}/${encodeURIComponent(name)}`);
}

/**
 * 导出全部 provider 为 YAML 文本。
 *
 * 这是唯一的非 JSON 端点：用 responseType:'text' 拿原始字符串。
 * client.ts 的响应拦截器对非 object 响应（字符串）原样放行，所以不会误走 code!==0 分支，
 * 也不能用 unwrap（unwrap 期望 {code,data,message}）。直接返回 res.data。
 */
export async function exportProviders(): Promise<string> {
  const res = await api.get(`${BASE}/export`, { responseType: 'text' });
  return res.data;
}

/** 按 YAML 文本导入 provider。strategy=merge 已存在则覆盖，replace 先清空再导入。 */
export async function importProviders(yaml: string, strategy: 'merge' | 'replace'): Promise<ImportResult> {
  return unwrap<ImportResult>(await api.post(`${BASE}/import`, { yaml, strategy }));
}

/** 预览：把 provider 的模型映射写入哪些执行器、各自生成什么文件内容，不落盘。 */
export async function previewProviderToExecutors(name: string, executorModels: ExecutorModels): Promise<PreviewEntry[]> {
  return unwrap<PreviewEntry[]>(await api.post(`${BASE}/${encodeURIComponent(name)}/preview`, { executor_models: executorModels }));
}

/** 应用：按预览结果实际写入各执行器配置文件，返回成功/失败列表。 */
export async function applyProviderToExecutors(name: string, executorModels: ExecutorModels): Promise<ApplyResult> {
  return unwrap<ApplyResult>(await api.post(`${BASE}/${encodeURIComponent(name)}/apply`, { executor_models: executorModels }));
}
