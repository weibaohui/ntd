# API Key 管理设计文档

> **设计日期**：2026-07-23
> **状态**：实现完成
> **关联 PR**：[#921](https://github.com/weibaohui/ntd/pull/921)（合并前）

---

## 1. 设计目标

统一管理各 AI 代码执行器的 API Key 等凭据，按需"灌入"到对应执行器的配置文件。

## 2. 架构总览

```
┌──────────────────────────────────────────────────┐
│  前端 (React + Ant Design)                       │
│  ├── API Key 管理面板（卡片式）                  │
│  │     ├── 新增 / 编辑 / 删除                     │
│  │     └── 应用按钮 → 三步弹窗                    │
│  │           ├── 步骤1：选择执行器 + 模型       │
│  │           ├── 步骤2：预览内容（Tab）          │
│  │           └── 步骤3：写入结果反馈             │
│  └── 执行器配置定义从 /api/v1/providers/          │
│      supported-executors 拉取                    │
└──────────────────────────────────────────────────┘
              ↓ HTTP (Axum Router)
┌──────────────────────────────────────────────────┐
│  后端 (Rust)                                     │
│  ├── ProfilesConfig（~/.ntd/profiles.yaml）       │
│  │     └── providers: HashMap<String, Provider>  │
│  ├── ProfileGeneratorRegistry                   │
│  │     ├── ClaudeCodeGenerator                  │
│  │     ├── PiGenerator                          │
│  │     ├── AtomCodeGenerator                    │
│  │     └── KiloGenerator                        │
│  └── 配置文件备份：同目录 .bak-{时间戳}（保留5份）│
└──────────────────────────────────────────────────┘
              ↓
┌──────────────────────────────────────────────────┐
│  各执行器配置文件                                │
│  ├── ~/.claude/settings.json                     │
│  ├── ~/.pi/agent/models.json + settings.json     │
│  ├── ~/.atomcode/config.toml                     │
│  └── ~/.kilo/config.json                         │
└──────────────────────────────────────────────────┘
```

## 3. 数据模型

```rust
/// 协议格式
pub enum Protocol { Openai, Anthropic }  // default: Openai

/// 单个模型条目
pub struct ProviderModel {
    pub name: String,                    // 模型标识符
    pub display_name: Option<String>,   // 显示名
    pub supports_1m_context: bool,      // 模型级 1M 上下文标记
}

/// 供应商（API 服务商）
pub struct Provider {
    pub name: String,                    // display_name
    pub api_key: String,
    pub base_url: String,
    pub protocol: Protocol,             // default: Openai
    pub models: Vec<ProviderModel>,
}

/// 完整 profiles.yaml 结构
pub struct ProfilesConfig {
    pub providers: HashMap<String, Provider>,
    pub current_profile: String,        // 保留字段，暂不使用
    pub profiles: HashMap<String, ...>,
}
```

> **关键决定**：曾尝试引入 Profile（多执行器配置模板）层，但用户反馈过度设计 — 实际场景不需要批量切换。Profile 字段保留以备扩展，但 UI 不再暴露。

## 4. 配置生成器设计

### 4.1 Trait 接口

```rust
pub trait ProfileGenerator: Send + Sync {
    fn executor_name(&self) -> &str;
    fn default_filename(&self) -> &str;
    fn config_path(&self, session_dir: &str) -> PathBuf;
    
    /// 渲染配置内容（不写盘）
    fn preview(&self, exec_ref: &ExecutorRef, provider: &Provider, 
               session_dir: &str) -> Result<(String, String), String>;
    
    /// 备份并写入
    fn generate(&self, exec_ref: &ExecutorRef, provider: &Provider, 
                session_dir: &str) -> Result<(), String>;
}
```

### 4.2 各生成器的真实格式

**ClaudeCodeGenerator** — `~/.claude/settings.json`
- Anthropic 协议 → 写 env 块：`ANTHROPIC_AUTH_TOKEN` / `ANTHROPIC_BASE_URL` / `ANTHROPIC_MODEL` 等
- OpenAI 协议 → 写顶层字段 `apiKey` / `baseUrl` / `model`
- 1M 模型：额外写 `ANTHROPIC_DEFAULT_SONNET_MODEL = {model}[1M]` 等

**PiGenerator** — `~/.pi/agent/models.json` + `~/.pi/agent/settings.json`
- 从读取的 models.json 提取 `providers` map，insert/更新当前 provider
- 协议 → `api`：`anthropic` → `"anthropic-messages"`, `openai` → `"openai-completions"`
- 写入所有模型到 `providers.{name}.models` 数组
- 更新 `settings.json` 的 `defaultProvider` / `defaultModel`

**AtomCodeGenerator** — `~/.atomcode/config.toml`
- 写入前先解析整个文件，删除已有的 `[providers.ntd-profile]` 段
- 追加新段：`type = "openai"` + model + base_url + api_key
- 保证可重复 apply

**KiloGenerator** — `~/.kilo/config.json`
- 完整覆写 snake_case JSON：`api_key` / `base_url` / `model` / `provider_type`

### 4.3 可重复性保证

| 执行器 | 重复 apply 行为 |
|--------|----------------|
| Claude Code | 完整覆写，幂等 |
| Kilo | 完整覆写，幂等 |
| PI | 用 provider name 作 key insert，替换原有 |
| AtomCode | 先删旧 `[providers.ntd-profile]` 段，再追加新段 |

## 5. API 设计

| 方法 | 路径 | 请求体 | 响应 |
|------|------|--------|------|
| GET | `/api/v1/providers` | — | 摘要列表（不含 api_key） |
| GET | `/api/v1/providers/supported-executors` | — | 执行器配置定义（含 config_path, has_generator） |
| GET | `/api/v1/providers/{name}` | — | 详情（含 api_key） |
| POST | `/api/v1/providers` | CreateProviderRequest | 摘要 |
| PUT | `/api/v1/providers/{name}` | UpdateProviderRequest | 摘要 |
| DELETE | `/api/v1/providers/{name}` | — | — |
| POST | `/api/v1/providers/{name}/preview` | `{ executor_models: {exec→model} }` | `[{executor, model, path, content}]` |
| POST | `/api/v1/providers/{name}/apply` | `{ executor_models: {exec→model} }` | `{ applied: [...], errors: [...] }` |
| POST | `/api/v1/providers/export` | `{}` | `text/yaml`（含 Content-Disposition 附件下载） |
| POST | `/api/v1/providers/import` | `{ yaml, strategy }` | `{ imported, skipped, errors }` |

## 6. 备份策略

每次 `generate()` 调用前：

1. 检查目标文件是否存在
2. 在**同目录**创建 `{原文件名}.bak-{时间戳}` 备份（不用集中备份目录）
3. 排序该目录下的 `{原文件名}.bak-*` 文件
4. 超过 5 份的最旧的自动删除

示例：
```
~/.atomcode/config.toml                    ← 当前
~/.atomcode/config.toml.bak-20260723_020000  ← 最新备份
~/.atomcode/config.toml.bak-20260722_190000  ← 较早备份
... (最多保留 5 份)
```

## 6.5 导入/导出

**目的**：跨机器迁移、团队共享、版本备份。

**导出流程**（`POST /api/v1/providers/export`）：
1. 后端调用 `ProfilesConfig::export_providers_to_yaml()`
2. 使用 serde_yaml 序列化 `providers` map，构造只含该段的 YAML
3. 加注释说明导出时间和导入方式
4. 返回 `Content-Disposition: attachment; filename="ntd-providers-YYYYMMDD.yaml"`
5. 前端用 Blob + a.click() 触发下载

**导入流程**（`POST /api/v1/providers/import`）：
1. 前端弹窗收集 YAML 文本（文件上传或粘贴）
2. 用户选策略：merge（默认） / replace
3. 后端先用 `serde_yaml::Value` 解析，校验顶层有 `providers:` 段
4. 逐个 provider 用 `Provider` 反序列化（错误只影响该项）
5. 内存态 `profiles.yaml` 直接覆盖 `providers` map（merge 策略下只 inset 覆盖）
6. 原子写回磁盘

**冲突策略**：
- `merge`（默认）：按 provider name 覆盖已存在的；不存在的则新增
- `replace`：先 `providers.clear()`，再用导入内容填充（UI 需二次确认）

**风险控制**：
- 导入仅触动 `providers` 段，不修改 `current_profile` 和 `profiles`，避免误改其他设置
- YAML 体经 serde_yaml 安全反序列化（无代码执行风险）
- 单个 provider 解析失败不影响其他（错误列表返回）

## 7. 移动端适配

- 检测 `useIsMobile()` 切换布局
- 卡片网格：桌面 3 列，平板 2 列，移动端 1 列
- 弹窗：移动端全屏宽（`width: 100%`，移除边距）
- 操作按钮：桌面 32px 高，移动端 24px
- 描述段落：移动端隐藏以省空间
- 代码预览 `<pre>`：暗色主题同步（基于 `useTheme()`）

## 8. 安全考虑

- API Key 明文存储在 `~/.ntd/profiles.yaml`
- 依赖文件系统权限保护（用户需 chmod 600）
- 本期不引入加密（YAGNI — 用户暂无此需求）
- 写入前自动备份可恢复到任意历史版本

## 9. 关键文件清单

| 文件 | 职责 |
|------|------|
| `backend/src/profiles.rs` | Provider 数据结构 + 加载/保存 |
| `backend/src/profiles/generators.rs` | 4 个生成器实现 + 备份逻辑 + all_executor_configs |
| `backend/src/handlers/profiles.rs` | API 端点（CRUD + preview + apply + supported-executors） |
| `frontend/src/components/settings/ProfilesPanel.tsx` | 卡片面板 + 三步应用弹窗 |

## 10. 后续扩展方向

- 增加 CodeBuddy / Opencode / Kimi 等其他执行器生成器
- 引入 AES 加密 API Key
- 支持 Profile 模板（一组配置批量应用到多执行器）
- 接入 Git 同步配置跨设备
- 检测各执行器配置文件的「其他自定义字段」并保留（深度合并而非覆写）
