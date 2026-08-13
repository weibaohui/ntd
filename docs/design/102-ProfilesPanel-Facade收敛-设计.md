# 102 - ProfilesPanel 直连 fetch 收敛为 providers Facade（ADR-005 / H5）

> 关联：[ADR-005](../decisions/ADR-005-重构与设计模式改进-决策.md) H5。一个 H 项一个设计文档、一个 PR。

## 1. 背景

`frontend/src/components/settings/ProfilesPanel.tsx`（API Key 管理面板）有 **11 处裸 `fetch(PROVIDERS_API)`**，`PROVIDERS_API = '/api/v1/providers'`。它们完全旁路 `utils/database/client.ts` 的 axios 实例，后果：

- **无重试**：`client.ts` 对 GET 网络错（无响应）做 3 次指数退避重试，裸 `fetch` 没有。
- **无统一解包**：`client.ts` 的响应拦截器对 `{code,data,message}` 统一校验 `code!==0` 并抽取错误信息；裸 `fetch` 每处手写 `await r.json()` + 各自的 `if(!r.ok)` / `if(j.code!==0)`，风格不一。
- **违反前端 13-禁止清单 #7**：「组件内直接使用 `fetch(url)`」。
- **更严重——假成功**：`createProvider`/`updateProvider`/`deleteProvider`/`preview`/`apply` 这 5 处**既不检查 `r.ok` 也不检查 `j.code`**，HTTP/业务失败时仍 `message.success('已创建')`。用户以为成功，实际后端已拒绝。

`utils/database/` 下已有 11 个同构 Facade（experts/skills/bots/sessions/…，模板见 `experts.ts`），**唯独缺 `providers.ts`**。本设计即补齐它。

## 2. 目标

新建 `frontend/src/utils/database/providers.ts` Facade（仿 `experts.ts`），把 11 处 fetch 收口为类型化函数；ProfilesPanel 仅消费 Facade，组件内 `fetch` 归零。**除一处修复（§5）外，行为逐条保持不变。**

## 3. Facade API（`providers.ts`）

| 函数 | 方法 + 路径 | 入参 | 返回 | 备注 |
|---|---|---|---|---|
| `listProviders` | GET `/api/v1/providers` | — | `ProviderSummary[]` | 后端只回 summary（无 api_key/models） |
| `getSupportedExecutors` | GET `/api/v1/providers/supported-executors` | — | `ExecutorConfigDef[]` | |
| `getProvider` | GET `/api/v1/providers/{name}` | `name` | `ProviderDetail` | `encodeURIComponent` |
| `createProvider` | POST `/api/v1/providers` | `body: ProviderInput` | `void` | |
| `updateProvider` | PUT `/api/v1/providers/{name}` | `name, body` | `void` | |
| `deleteProvider` | DELETE `/api/v1/providers/{name}` | `name` | `void` | |
| `exportProviders` | GET `/api/v1/providers/export` | — | `string`（YAML） | **不走 `unwrap`**：`responseType:'text'`，直接返回 `res.data` |
| `importProviders` | POST `/api/v1/providers/import` | `yaml, strategy` | `{imported, errors}` | |
| `previewProviderToExecutors` | POST `/api/v1/providers/{name}/preview` | `name, executorModels` | `PreviewEntry[]` | |
| `applyProviderToExecutors` | POST `/api/v1/providers/{name}/apply` | `name, executorModels` | `ApplyResult` | |

实现要点：
- 有返回值的函数统一 `return unwrap(await api.<method>(url, [body]))`，与 `experts.ts` 一致。
- `createProvider`/`updateProvider`/`deleteProvider` 返回 `void`，只 `await api.<method>(...)` 不走 `unwrap`——`client.ts` 响应拦截器已对 `code!==0`/非 2xx 自动 reject，§5 的假成功修复由此成立，无需再解包 data（与 `experts.ts` 的 `deleteExpert` 等 void 函数一致）。
- preview/apply 的请求体用 providers.ts 内部别名 `type ExecutorModels = Record<string, string>`（执行器→模型名映射），对应后端 `executor_models` 字段。
- `exportProviders`：`api.get(BASE + '/export', { responseType: 'text' })`。`client.ts` 拦截器对非 object 响应（字符串）原样放行，故不会误走 `code!==0` 分支；返回 `res.data`（YAML 文本）。

## 4. 类型归位

新建 `frontend/src/types/provider.ts`（仿 `types/expert.ts`），把现内联在 ProfilesPanel 的类型迁出，并补齐后端实际返回的字段：

```ts
export type ProviderProtocol = 'openai' | 'anthropic';
export interface ProviderModel { name: string; display_name?: string; supports_1m_context?: boolean; }
export interface ProviderSummary { name: string; display_name: string; base_url: string; protocol: ProviderProtocol; model_count: number; }
export interface ProviderDetail { name: string; display_name: string; api_key: string; base_url: string; protocol: ProviderProtocol; models: ProviderModel[]; }
export interface ExecutorConfigDef { name: string; display_name: string; config_path: string; has_generator: boolean; }
export interface ProviderInput { name?: string; display_name: string; api_key: string; base_url: string; protocol: ProviderProtocol; models: ProviderModel[]; }
export interface PreviewEntry { executor: string; model: string; path: string; content: string; }
export interface ApplyResult { applied: string[]; errors: string[]; }
export interface ImportResult { imported: string[]; errors: string[]; }
```

Facade 与组件都从 `@/types/provider` 导入（前端强制 `@/`）。

## 5. ⚠️ 唯一行为变更（修复，非回归）

`createProvider`/`updateProvider`/`deleteProvider`/`previewProviderApply`/`applyProvider` 原先不校验响应，失败仍显示成功。走 Facade 后：

- axios 对非 2xx 自动 reject；响应拦截器对 `code!==0` reject（携带后端 `message`）。
- 拒绝进入组件**既有**的 `catch (err)` → `message.error('操作失败: ' + err.message)`。

这正是 H5 列明的「无 unwrap」问题修复，符合 13-禁止清单 #6「空 catch / 吞错」。**不视为回归**——原行为是 bug。

## 6. 行为不变式（除 §5 外逐条保持）

- 端点、HTTP 方法、请求体、成功路径的数据提取逐条不变。
- **N+1 保持**：`load()` 先 `listProviders()` 取 summary 列表，再对每个 `name` 并发 `getProvider()` 取 detail（后端 list 不含 api_key/models，组件需要它们做 `maskKey` 与模型 Tag 展示）。
- 组件 UI 流程、loading/弹窗状态机、`message` 文案、`maskKey` 等纯函数不变。
- 公开组件签名 `ProfilesPanel()` 零改动（无 props，调用方不受影响）。

## 7. 错误处理统一（净收益）

- 删除手写 `JSON.stringify(body)` + `headers: {'Content-Type':...}`（axios 默认）。
- 删除手写 `if(!r.ok)` / `if(j.code!==0)`（拦截器 + `unwrap` 统一）。
- 所有 GET 自动获得 3 次指数退避重试（`client.ts` 既有）。

## 8. 测试（前端 11-测试规范，vitest）

新增 `frontend/src/utils/database/providers.test.ts`，命名 `test_<函数>_<场景>`。`vi.mock('@/utils/database/client')` 注入假 `api`/`unwrap`，覆盖：

- 每函数以正确 `method`/`URL`/`body` 调用（如 `test_getProvider_对name做encodeURIComponent`）。
- 成功返回 `unwrap` 的结果（`test_listProviders_解包data数组`）。
- `code!==0` 时抛错（`test_createProvider_后端拒绝时抛错不假成功`——覆盖 §5 修复）。
- `exportProviders` 走 text 通路、不被 `unwrap`（`test_exportProviders_返回YAML文本不走unwrap`）。

组件层不新增 vitest（组件渲染依赖 antd/主题，按既有约定以 Playwright 冒烟覆盖）。

## 9. 验收清单

- [ ] `cd frontend && npx tsc --noEmit` 零错误。
- [ ] `cd frontend && npm run build` 零新告警。
- [ ] `grep -n 'fetch(' frontend/src/components/settings/ProfilesPanel.tsx` → 0 命中。
- [ ] vitest 新增用例全过；既有用例不回归。
- [x] Playwright 冒烟：API Key 面板加载 / 新增→删除 / 导出 流程正常（`frontend/tests/verify_api_key_panel.spec.ts`，证据发 PR 评论，不提交截图）。

## 10. 文件清单

- 新增 `frontend/src/utils/database/providers.ts` + `providers.test.ts`
- 新增 `frontend/src/types/provider.ts`
- 改 `frontend/src/utils/database/index.ts`（barrel `export * from './providers'`）
- 改 `frontend/src/components/settings/ProfilesPanel.tsx`（删内联类型 + 11 处 fetch，改用 Facade；组件签名不变）
- 新增 `frontend/tests/verify_api_key_panel.spec.ts`（Playwright 冒烟：加载 / 新增→删除 / 导出 三个用例）
