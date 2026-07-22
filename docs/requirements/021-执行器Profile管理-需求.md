# 0. 文件修改记录表

| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| AI | 2026-07-22 | 初始版本 |

# 1. 背景（Why）

当前 ntd 支持 13 种 AI 代码执行器（Claude Code、PI、AtomCode、Kilo 等），但各执行器的 **API Key、模型选择、Base URL** 等凭据配置各自散落在各执行器自己的配置文件中（`~/.claude/settings.json`、`~/.pi/config.yaml` 等），无集中管理入口。

用户痛点：
- 换 Key 需要逐个修改各文件，容易遗漏
- 不同项目使用不同 API Key / 模型，当前无"切换"概念
- 设置页面仅支持配路径和模型名，无 API Key 管理能力

# 2. 目标（What，必须可验证）

- [ ] 实现 `profiles.yaml` 集中存储所有执行器的 API Key 等凭据
- [ ] 实现 Config Generator 模式，将 Profile 数据生成为各执行器的原生配置文件格式
- [ ] 首批支持 Claude Code、PI、AtomCode、Kilo 四个执行器的配置生成
- [ ] 提供 Profile CRUD API 接口
- [ ] 提供配置切换（apply）API，切换时自动备份原配置文件
- [ ] 前端设置页新增「API Key」Tab，支持可视化管理
- [ ] 所有代码通过 `cargo clippy -- -D warnings` + `cargo test`

# 3. 非目标（Explicitly Out of Scope）

- 不实现加密存储 API Key（依赖文件系统权限，后续版本可引入 AES 加密）
- 不实现导入/导出 Profile（后续版本）
- 不实现其余 9 个执行器的生成器（CodeBuddy、Opencode、Kimi、Mimo 等——后续版本逐步添加）
- 不在本次实现 CLI 命令（后续按需添加）
- 不修改现有执行器的适配器层代码

# 4. 使用场景 / 用户路径

## 场景一：配置新 API Key

1. 用户打开 ntd Web 界面 → 点击左侧导航「设置」→ 进入「API Key」Tab
2. 点击「新建 Profile」→ 填写标识符和显示名称
3. 创建成功后，在 Profile 编辑界面填入各执行器的 API Key
4. 点击「切换」按钮 → 系统生成各执行器的配置文件并写入对应路径

## 场景二：切换环境

1. 用户有多个 Profile（如 "日常开发"、"项目 A"、"项目 B"）
2. 在 API Key 管理页面，点击目标 Profile 对应的「切换」按钮
3. 系统自动备份当前各执行器的配置文件
4. 写入新 Profile 的配置
5. 前端显示切换结果（成功/跳过/失败）

## 场景三：删除不再使用的 Profile

1. 在 API Key 管理页面找到目标 Profile
2. 点击「删除」按钮（当前激活的 Profile 不可删除）
3. 确认后删除

# 5. 功能需求清单（Checklist）

## 5.1 后端数据层

- [ ] `ProfilesConfig` 数据结构定义（含顶层配置、Profile 列表、执行器设置）
- [ ] `ExecutorSettings` 通用字段（api_key / model / base_url）+ 专有字段兜底
- [ ] `profiles.yaml` 加载（不存在时创建默认值）
- [ ] 原子写保存（临时文件 + rename）

## 5.2 后端配置生成器

- [ ] `ProfileGenerator` trait 定义
- [ ] `ProfileGeneratorRegistry` 注册表
- [ ] `ClaudeCodeGenerator` → `~/.claude/settings.json`
- [ ] `PiGenerator` → `~/.pi/config.yaml`
- [ ] `AtomCodeGenerator` → `~/.atomcode/settings.json`
- [ ] `KiloGenerator` → `~/.kilo/config.json`
- [ ] 覆写前自动备份原文件到 `~/.ntd/profile_backups/`
- [ ] 备份保留最近 10 份

## 5.3 后端 API

- [ ] `GET /api/v1/profiles` 列表
- [ ] `GET /api/v1/profiles/current` 当前详情
- [ ] `POST /api/v1/profiles` 创建
- [ ] `PUT /api/v1/profiles/{name}` 更新
- [ ] `DELETE /api/v1/profiles/{name}` 删除
- [ ] `POST /api/v1/profiles/{name}/apply` 切换应用

## 5.4 前端 UI

- [ ] Profile 列表表格（名称、描述、执行器数量、当前状态）
- [ ] 新建 Profile 弹窗（标识符/显示名称/描述）
- [ ] 编辑 Profile 弹窗（显示名称/描述/当前配置预览）
- [ ] 删除确认（Popconfirm，当前激活不可删）
- [ ] 切换按钮 + 结果展示 Modal（成功/跳过/失败列清单）
- [ ] 设置页新增「API Key」Tab

# 6. 约束条件（非常关键）

## 技术约束

- Rust 后端不得引入新的大依赖（`serde_yaml` 和 `serde_json` 已存在）
- 生产代码禁止 `.unwrap()` / `.expect()` / `panic!`
- 单个函数体不超过 30 行

## 架构约束

- Profile 路由挂载在 `/api/v1/profiles` 下
- 与现有 `~/.ntd/config.yaml` 分开，独立存储为 `~/.ntd/profiles.yaml`
- 生成器只写各执行器的公认字段，不写实验性/未文档化字段
- 切换时通过 `current_profile` 字段标记，状态持久化在 profiles.yaml

## 安全约束

- API Key 明文存储，依赖 `~/.ntd/` 目录权限保护（用户需确保 chmod 600）
- 覆写前自动备份原配置

# 7. 可修改 / 不可修改项

- ❌ 不可修改：
  - 现有的 `ExecutorDef` 静态数组（`adapters/mod.rs`）
  - 现有的执行器适配器（`adapters/pi.rs` 等 CLI 参数构造逻辑）
  - 现有的 `Config::load()` 配置加载流程
  - 现有的 `handlers/config.rs` 配置 API

- ✅ 可调整：
  - 新增执行器生成器时可在 `ProfileGeneratorRegistry` 注册
  - 生成器写入的字段可根据各执行器实际需求调整
  - 前端 UI 样式可以按需修改

# 8. 接口与数据约定

## profiles.yaml 格式

```yaml
current_profile: default
profiles:
  default:
    name: 默认配置
    description: 日常开发使用的默认配置
    executors:
      claudecode:
        api_key: sk-ant-xxxxxxxx
        model: claude-sonnet-4-20250514
      pi:
        api_key: sk-xxx
        anthropic_api_key: sk-ant-xxx
```

## API 响应格式

所有 API 响应使用现有的 `ApiResponse<T>` 包装：

```json
{
  "code": 0,
  "data": { ... },
  "message": "ok"
}
```

## Apply 响应

```json
{
  "profile_name": "default",
  "profile_display_name": "默认配置",
  "applied_executors": ["claudecode", "pi"],
  "skipped_executors": ["kilo"],
  "errors": []
}
```

# 9. 验收标准（Acceptance Criteria）

- 如果创建 Profile 后通过 API 能查到，则 CRUD 功能通过
- 如果 apply Profile 后 `~/.claude/settings.json` 被正确写入，则 Claude Code 生成器通过
- 如果 apply Profile 后 `~/.pi/config.yaml` 被正确写入，则 PI 生成器通过
- 如果 apply Profile 时原配置文件被备份到 `~/.ntd/profile_backups/`，则备份功能通过
- 如果使用 `cargo clippy -- -D warnings` 零告警，则代码质量通过
- 如果 `cargo test` 所有用例通过，则功能正确性通过

# 10. 风险与已知不确定点

| 风险 | 缓解措施 |
|------|----------|
| 各执行器的配置格式可能随版本变化 | 生成器只写稳定公认字段，字段名通过 serde_json::Map 动态构建 |
| API Key 明文存储在磁盘上 | 依赖 `~/.ntd/` 目录权限；后续版本引入加密 |
| 执行器配置文件被覆盖后可能不兼容 | apply 前自动备份，用户可手动恢复 |

# 11. 非目标（重复申明）

- 其余 9 个执行器的配置生成器不在首发范围
- 不支持批量导入/导出 Profile
- 不支持 API Key 加密存储
- 不涉及执行器适配器改造
