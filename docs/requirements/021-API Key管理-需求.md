# 0. 文件修改记录表

| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| AI | 2026-07-23 | 初始版本（基于多次迭代整理） |

# 1. 背景（Why）

用户的电脑上有 N 个 AI 代码执行器（Claude Code、PI、AtomCode、Kilo 等 13 个），每个执行器各自读取自己的配置文件（`~/.claude/settings.json`、`~/.pi/agent/models.json` 等）。这些文件里的 **API Key / Base URL / 模型** 配置分散在各处：

- 换 Key 要逐个文件手动改
- 不同项目用不同 Key 时没有快速切换机制
- 设置页面只暴露路径，无法管理凭据

需求：管理 AI 服务凭据，按需将一组配置"灌入"到选中的执行器配置文件中。

# 2. 目标（What，必须可验证）

- [ ] 集中管理 API Key、Base URL、协议格式、模型列表
- [ ] 每个 API Key 一张卡片的视觉化卡片布局
- [ ] 应用时选择执行器列表，选择模型，预览内容，确认后写入配置文件
- [ ] 写入前自动备份原文件为同目录 `.bak-<时间戳>`，保留最近 5 份
- [ ] 支持移动端布局（卡片列表 + 全屏弹窗）
- [ ] 全部代码通过 cargo clippy -- -D warnings + cargo test

# 3. 非目标（Explicitly Out of Scope）

- 不实现 API Key 加密存储（明文 + 文件系统权限）
- 不实现其余 9 个执行器的生成器（首批 4 个）
- 不实现 Profile 配置文件批量切换（用户偏好按需单点写入）
- 不实现云同步/团队共享
- 不实现导入/导出功能
- 不修改现有执行器适配器代码

# 4. 使用场景 / 用户路径

**场景 A — 首次配置 API Key**
1. 进入设置 → API Key Tab
2. 点「新增 API Key」→ 填写：标识符、显示名、API Key、Base URL、协议（OpenAI 兼容 / Anthropic 原生）、模型列表
3. 保存后卡片展示在列表中
4. 准备应用

**场景 B — 应用 API Key 到一个或多个执行器**
1. 卡片上点「应用」
2. 弹窗步骤 1：勾选要应用的执行器，每个执行器选模型（默认第一个）
3. 点「下一步」→ 步骤 2：每个执行器一个 Tab，展示路径 + 文件内容预览
4. 点「确认写入」→ 写入结果反馈
5. 如有问题可返回重新选择

**场景 C — 编辑/删除 API Key**
1. 卡片右上角编辑图标 → 修改各字段并保存
2. 删除图标 → 确认后删除

**场景 D — 导入/导出 API Key**
1. 卡片列表顶部点「导出」→ 浏览器下载 `ntd-providers-YYYYMMDD.yaml` 文件（含所有 Provider）
2. 卡片列表顶部点「导入」→ 弹窗里选 YAML 文件或粘贴文本，选择合并/替换策略，确认后写入
3. merge 策略：按 provider 名称覆盖已存在的，已不存在的则新增
4. replace 策略：先清空所有 Provider，再导入（强制要求二次确认）

**场景 E — 备份恢复**
- 每次写入会自动备份原配置文件为 `{原文件名}.bak-{时间戳}`
- 超 5 份的自动清理

# 5. 功能需求清单（Checklist）

## 5.1 后端：供应商（Provider）数据模型

- [x] `Provider` 包含 name / display_name / api_key / base_url / protocol / models[]
- [x] `Protocol` 枚举：`openai` / `anthropic`
- [x] `ProviderModel` 包含 name / display_name / supports_1m_context
- [x] `Protocol::default()` = `Openai`
- [x] 统一存储在 `~/.ntd/profiles.yaml`（dev 模式为 `profiles.dev.yaml`）

## 5.2 后端：配置生成器

- [x] `ProfileGenerator` trait：`executor_name()` / `default_filename()` / `preview()` / `generate()`
- [x] `ProfileGeneratorRegistry` 注册表
- [x] 实现 4 个生成器：ClaudeCodeGenerator / PiGenerator / AtomCodeGenerator / KiloGenerator
- [x] `preview()` 返回 `(file_path, file_content)` 不写盘
- [x] `generate()` 写入磁盘：备份 → 写文件

### 各生成器的配置文件格式

| 执行器 | 目标文件 | 协议映射 |
|--------|---------|----------|
| Claude Code | `~/.claude/settings.json` | Anthropic → env 块 / OpenAI → 顶层字段 |
| PI | `~/.pi/agent/models.json` + `~/.pi/agent/settings.json` | `anthropic-messages` / `openai-completions` |
| AtomCode | `~/.atomcode/config.toml` | 追加 `[providers.ntd-profile]` 段，先去旧段 |
| Kilo | `~/.kilo/config.json` | snake_case |

## 5.3 后端：备份机制

- [x] `backup_existing_config` 在原文件同目录创建 `.bak-<时间戳>` 备份
- [x] 保留最近 5 份备份，超过的自动清理

## 5.4 后端：API 端点

- [x] `GET /api/v1/providers` — 列表
- [x] `GET /api/v1/providers/supported-executors` — 执行器配置定义（name, display_name, config_path, has_generator）
- [x] `GET /api/v1/providers/{name}` — 详情（含 api_key）
- [x] `POST /api/v1/providers` — 创建
- [x] `PUT /api/v1/providers/{name}` — 更新
- [x] `DELETE /api/v1/providers/{name}` — 删除
- [x] `POST /api/v1/providers/{name}/preview` — 预览（请求体：`executor_models: {exec: model}`）
- [x] `POST /api/v1/providers/{name}/apply` — 应用（请求体：同上）
- [x] `GET /api/v1/providers/export` — 导出所有 Provider 为 YAML 文本（Content-Disposition: attachment）
- [x] `POST /api/v1/providers/import` — 导入（请求体：`{yaml, strategy}`；strategy = merge/replace）

## 5.5 后端：执行器配置定义

- [x] `all_executor_configs()` 从 `adapters/mod.rs` 的 EXECUTORS 数组 + ProfileGeneratorRegistry 组合
- [x] 每个执行器定义含 name / display_name / config_path / has_generator

## 5.6 前端：API Key 管理面板

- [x] 卡片网格布局，每张卡显示：显示名 + 协议标签 + Key 脱敏 + URL + 模型列表
- [x] 卡片右上角：编辑 / 删除 按钮
- [x] 卡片底部：「应用」主按钮
- [x] 「新增 API Key」按钮（右上角）
- [x] 「刷新」按钮
- [x] 协议格式下拉框二选一（OpenAI 兼容 / Anthropic 原生）
- [x] 模型列表编辑：每行有标识 + 显示名 + 1M 上下文开关 + 删除
- [x] 「导出」按钮 — 调 `POST /api/v1/providers/export`，浏览器下载 `ntd-providers-YYYYMMDD.yaml`
- [x] 「导入」按钮 + 弹窗 — 文件上传 / 文本框粘贴，单选合并/替换策略，调 `POST /api/v1/providers/import`

## 5.7 前端：应用弹窗（三步流程）

- [x] 步骤条：选择执行器 → 预览配置 → 完成
- [x] 步骤 1 — 选择执行器：勾选每行，下拉选模型，自动显示路径
- [x] 步骤 2 — 预览：每个执行器一个 Tab，显示文件路径和内容
- [x] 步骤 3 — 完成：写入结果反馈
- [x] 全部支持暗色主题（代码预览块背景/文字颜色自适应）

# 6. 约束条件

## 技术约束

- Rust 后端不得引入新依赖（serde_yaml / serde_json 已存在）
- 生产代码禁止 `.unwrap()` / `.expect()` / `panic!`
- 单个函数体不超过 30 行
- cargo clippy -- -D warnings 零告警
- 前端 TypeScript 零错误
- 仅 4 个执行器支持生成器（Claude Code、PI、AtomCode、Kilo）

## 安全约束

- API Key 明文存储 `~/.ntd/profiles.yaml`
- 依赖文件系统权限（chmod 600）保护
- 写入前自动备份，最近 5 份轮转

## 架构约束

- 配置路径定义唯一来源：`backend/src/profiles/generators.rs` 的 `all_executor_configs()`
- 配置文件内容格式与各执行器真实使用的格式严格匹配（不臆造）
- 重复 apply 不重复追加（AtomCode 先删旧 `[providers.ntd-profile]` 段）

# 7. 可修改 / 不可修改项

- ❌ **不可修改**：
  - 现有的 `ExecutorDef` 静态数组（`adapters/mod.rs`）
  - 现有的执行器适配器层 CLI 参数构造逻辑
  - 现有的 `Config::load()` 和 `~/.ntd/config.yaml` 配置加载
- ✅ **可调整**：
  - 新增执行器生成器
  - 生成器输出的字段（按各执行器需求）
  - 前端样式
  - 协议 ↔ api type 映射规则

# 8. 接口与数据约定

## profiles.yaml 格式

```yaml
providers:
  deepseek-anthropic:
    name: DeepSeek (Anthropic 协议)
    api_key: sk-xxx
    base_url: https://api.deepseek.com/anthropic
    protocol: anthropic
    models:
      - name: deepseek-v4-flash
        display_name: DeepSeek v4 Flash
        supports_1m_context: true
      - name: deepseek-v4-pro
        supports_1m_context: true
current_profile: default
profiles:
  default: { name: 默认配置, description: ..., executors: {} }
```

## API 响应：ApiResponse<T> 标准包装

```json
{ "code": 0, "data": ..., "message": "ok" }
```

## 应用请求体

```json
{
  "executor_models": {
    "claudecode": "deepseek-v4-flash",
    "atomcode": "deepseek-v4-flash"
  }
}
```

## Apply 响应

```json
{
  "applied": ["claudecode (deepseek-v4-flash)", "atomcode (deepseek-v4-flash)"],
  "errors": []
}
```

## 导出 YAML 格式

导出文件 `ntd-providers-YYYYMMDD.yaml` 内容：

```yaml
# ntd API Key export
# 包含所有 Provider（API Key、Base URL、协议、模型列表）
# 导入：POST /api/v1/providers/import body={"yaml":"<此处内容>","strategy":"merge"}

providers:
  <provider_name>:
    name: <display_name>
    api_key: sk-xxx
    base_url: https://...
    protocol: <openai|anthropic>
    models:
      - name: <model_id>
        display_name: <可选>
        supports_1m_context: <bool>
```

## 导入请求/响应

```json
// POST /api/v1/providers/import
{ "yaml": "<整个 YAML 文本>", "strategy": "merge" }
// 响应
{ "imported": ["a", "b"], "skipped": [], "errors": [] }
```

# 9. 验收标准

- 创建 / 编辑 / 删除 API Key 后通过 `GET /api/v1/providers` 能查到
- `GET /api/v1/providers/{name}` 返回完整详情（含 api_key）
- `GET /api/v1/providers/supported-executors` 返回 13 个执行器定义，4 个 `has_generator: true`
- 选中多个执行器 + 每个选不同模型 → 预览每个 Tab 标题格式 `{exec_name} — {model_name}`，内容正确
- 确认写入后，对应配置文件被改写；同名 `.bak-{时间戳}` 备份出现
- 多次 apply AtomCode，文件中 `[providers.ntd-profile]` 段不重复
- 移动端访问：卡片列表变为单列，弹窗全屏宽，代码块暗色主题适配

# 10. 风险与已知不确定点

| 风险 | 缓解措施 |
|------|----------|
| Claude Code / Kilo 完整覆写会丢失用户已有字段 | 已知问题，备份文件可恢复；后续可定制 |
| PI 替换 provider 时可能丢失用户自定义字段（如 maxTokens） | 已知问题，备份可恢复 |
| 配置文件备份未做版本校验 | 用户需自己检查 `.bak-` 文件 |
| 其他 9 个执行器（CodeBuddy/Opencode/Kimi 等）无法生成 | 用户后续可追加生成器 |

# 11. 非目标（重复申明）

- 不实现其余 9 个执行器的生成器
- 不支持加密存储
- 不涉及执行器适配器层代码改造
- 不提供 Profile 批量切换（仅按需单点 apply）
