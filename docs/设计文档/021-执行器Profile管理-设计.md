# 执行器 Profile 管理设计文档

> **设计日期**：2026-07-22
> **状态**：初稿待确认
> **关联 Issue**：无（新增功能）

---

## 1. 背景与动机

当前 ntd 支持 13 种 AI 代码执行器（Claude Code、PI、AtomCode、Kilo、Kimi、Mimo 等），但这些执行器的**API Key、模型选择、base_url** 等配置各自存储在各执行器自己的配置文件中，散落在不同路径：

| 执行器 | 配置文件路径 | 格式 |
|--------|-------------|------|
| Claude Code | `~/.claude/settings.json` | JSON |
| PI | `~/.pi/config.yaml` | YAML |
| AtomCode | `~/.atomcode/` | JSON/YAML |
| CodeBuddy | `~/.codebuddy/` | JSON |
| Opencode | `~/.opencode/` | JSON |
| Kilo | `~/.kilo/` | JSON |
| Kimi | `~/.kimi/` | JSON |
| Mimo | `~/.local/share/mimocode/` | JSON |

用户痛点：

1. **API Key 散落各文件** — 换 Key 需要逐个修改，容易遗漏
2. **无法快速切换** — 不同项目使用的 API Key / 模型不同，当前无"切换"概念
3. **无统一管理入口** — 设置页可配路径/模型，但无凭据管理

## 2. 设计目标

- **API Key 统一管理** — 所有执行器的 API Key 集中存储在 `~/.ntd/profiles.yaml`
- **配置生成器** — 从统一 Profile 生成各执行器原生格式的配置文件，写入对应路径
- **Profile 切换** — 一键切换整套配置（即"环境切换"）
- **渐进式** — 先支持最常用的执行器（Claude Code、PI、AtomCode、Kilo），后续扩展

## 3. 数据结构设计

### 3.1 profiles.yaml 顶层结构

```yaml
# ~/.ntd/profiles.yaml
# 当前激活的 profile 名称
current_profile: default

# 所有 profile 的定义
profiles:
  default:
    name: 默认配置
    description: 日常开发使用
    # 各执行器配置（key 为 executor type name）
    claudecode:
      api_key: sk-ant-xxxxxxxx
      base_url: https://api.anthropic.com
    pi:
      openai_api_key: sk-xxxxxxxx
      anthropic_api_key: sk-ant-xxxxxxxx
    atomcode:
      api_key: sk-ant-xxxxxxxx
```

### 3.2 Rust 数据结构

```rust
/// 顶层 Profile 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfilesConfig {
    pub current_profile: String,
    pub profiles: HashMap<String, ExecutorProfile>,
}

/// 单个 Profile
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutorProfile {
    pub name: String,
    pub description: Option<String>,
    #[serde(default)]
    pub claudecode: Option<ExecutorSettings>,
    #[serde(default)]
    pub pi: Option<ExecutorSettings>,
    #[serde(default)]
    pub atomcode: Option<ExecutorSettings>,
    // ... 每种执行器
}

/// 通用执行器设置（各执行器共享的字段）
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExecutorSettings {
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub model: Option<String>,
    /// 额外扩展字段 — 特定执行器的专有配置以 HashMap 兜底
    #[serde(default, flatten)]
    pub extra: HashMap<String, String>,
}
```

### 3.3 设计理由

- **扁平字段 + HashMap 兜底**：各执行器的 API Key 字段名不同（Claude Code 是 `apiKey`、PI 是按 provider 分的 `openai_api_key`/`anthropic_api_key`），用 `#[serde(flatten)]` 让通用字段显式建模、专有字段走 HashMap 不丢数据
- **Option 而非 required**：一个 Profile 可以只配某几个执行器，不配的跳过
- **名称对齐**：profile 的 key 与 `ExecutorType.as_str()` 一致，便于查找

## 4. 核心模块设计

### 4.1 模块划分

```
backend/src/
├── profiles.rs                # ProfilesConfig 数据结构 + load/save
├── profiles/
│   ├── mod.rs                 # 模块入口
│   └── generators.rs          # 各执行器配置生成器
└── handlers/
    └── profiles.rs            # HTTP API handlers
```

### 4.2 ProfilesConfig — 加载/保存

- 路径：`~/.ntd/profiles.yaml`
- `load()` — 读取文件，不存在则返回带 `default` 空 profile 的默认值
- `save()` — 原子写（临时文件 + rename）
- 内存：由 `AppState` 持有，使用 `Arc<RwLock<ProfilesConfig>>`

### 4.3 Config Generators（生成器模式）

每个执行器实现一个生成器，将 `ExecutorSettings` 转换为其原生配置文件格式并写入目标路径：

```rust
pub trait ProfileGenerator {
    /// 该生成器对应的执行器名称
    fn executor_name(&self) -> &str;
    
    /// 目标配置文件的路径
    fn config_path(&self) -> PathBuf;
    
    /// 将 profile settings 生成为目标格式并写入文件
    fn generate(&self, settings: &ExecutorSettings) -> Result<(), String>;
}
```

首批实现的生成器：

| 执行器 | 生成器 | 目标文件 | 写入内容 |
|--------|--------|----------|----------|
| Claude Code | `ClaudeCodeGenerator` | `~/.claude/settings.json` | JSON: `{ "apiKey": "...", "model": "..." }` |
| PI | `PiGenerator` | `~/.pi/config.yaml` | YAML: provider API keys |
| AtomCode | `AtomCodeGenerator` | `~/.atomcode/settings.json` | JSON |
| Kilo | `KiloGenerator` | `~/.kilo/config.json` | JSON |

### 4.4 API 路由

所有路由挂载在 `v1` 下（与当前 API 路由设计一致）：

| 方法 | 路径 | 说明 |
|------|------|------|
| `GET` | `/api/v1/profiles` | 获取所有 profile 列表 |
| `GET` | `/api/v1/profiles/current` | 获取当前 profile 详情 |
| `POST` | `/api/v1/profiles` | 创建新 profile |
| `PUT` | `/api/v1/profiles/:name` | 更新指定 profile |
| `DELETE` | `/api/v1/profiles/:name` | 删除指定 profile |
| `POST` | `/api/v1/profiles/:name/apply` | 应用（切换）指定 profile → 写配置文件 |
| `GET` | `/api/v1/profiles/:name/diff` | 预览切换后的配置差异 |

### 4.5 CLI 命令

在现有的 `ntd exec` 命令下新增子命令：

```bash
# 列出所有 profile
ntd exec profile list

# 查看当前 profile
ntd exec profile current

# 创建 profile（交互式或参数）
ntd exec profile create <name> --api-key <key> --model <model>

# 切换并应用 profile
ntd exec profile switch <name>

# 应用当前 profile（重写各执行器配置文件）
ntd exec profile apply

# 删除 profile
ntd exec profile delete <name>
```

## 5. 实现计划

### Phase 1（本次实现）

1. 后端数据结构 + `profiles.yaml` 加载/保存（`profiles.rs`）
2. 列出/切换 profile 的 API 路由
3. 首批 4 个执行器的配置生成器：Claude Code、PI、AtomCode、Kilo
4. CLI 命令：`ntd exec profile list|switch|apply`
5. 前端 Profile 管理面板（设置页 Tab）

### Phase 2（后续扩展）

- 其余执行器的生成器（CodeBuddy、Opencode、Kimi、Mimo 等）
- 支持 secret 加密存储（避免 API Key 明文）
- 导入/导出 profile（用于团队共享）

## 6. 安全考虑

- API Key 以明文存储在 `~/.ntd/profiles.yaml` 中，依赖文件系统权限（600）保护
- 后续可引入 AES 加密存储
- 写入各执行器配置文件的 Key 同样为明文（与执行器自身文件权限一致）
- 应用 profile 时记录操作日志到 `execution.log`

## 7. 风险与缓解

| 风险 | 缓解 |
|------|------|
| 覆盖执行器已有配置 | apply 前先备份原文件到 `~/.ntd/profile_backups/` |
| 格式变化导致执行器不启动 | 生成器仅写公认字段，不写实验性/未文档化字段 |
| 多个 profile 操作并发 | ProfilesConfig 由 RwLock 保护，写操作独占 |

---

## 附录：各执行器配置文件格式调研

### Claude Code — `~/.claude/settings.json`

```json
{
  "apiKey": "sk-ant-xxxxxxxx",
  "model": "claude-sonnet-4-20250514"
}
```

### PI — `~/.pi/config.yaml`

```yaml
openai_api_key: sk-xxxx
anthropic_api_key: sk-ant-xxxx
google_api_key: xxxx
default_model: jiutian/deepseek/deepseek-v4-flash
```

### AtomCode — `~/.atomcode/settings.json`

类似 Claude Code 格式：
```json
{
  "apiKey": "sk-ant-xxxxxxxx"
}
```

### Kilo — `~/.kilo/config.json`

```json
{
  "api_key": "xxx",
  "model": "claude-3-5-sonnet"
}
```
