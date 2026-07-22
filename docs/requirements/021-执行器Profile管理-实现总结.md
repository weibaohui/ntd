# 0. 文件修改记录表

| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| AI | 2026-07-22 | 初始版本 |

# 1. 实现概述

为 ntd 实现了**执行器 Profile 管理系统**，统一管理各 AI 代码执行器的 API Key 等凭据配置。

核心思路：通过 `~/.ntd/profiles.yaml` 集中存储凭据，利用 Config Generator 模式将 Profile 数据转换为各执行器原生格式的配置文件，写入对应路径实现一键切换。

# 2. 对应需求

- **需求文档**：`docs/requirements/021-执行器Profile管理-需求.md`
- **设计文档**：`docs/设计文档/021-执行器Profile管理-设计.md`
- **测试文档**：`docs/testing/021-执行器Profile管理-测试.md`

# 3. 实现功能清单

## 3.1 已完成

| 功能 | 状态 | 说明 |
|------|------|------|
| 后端数据结构 | ✅ | `ProfilesConfig`、`ExecutorProfile`、`ExecutorSettings` |
| profiles.yaml 加载/保存 | ✅ | 不存在自动创建默认值，原子写（temp + rename） |
| ProfileGenerator trait | ✅ | 标准接口定义 + 自动备份原文件 |
| Claude Code 生成器 | ✅ | `ClaudeCodeGenerator` → `~/.claude/settings.json` |
| PI 生成器 | ✅ | `PiGenerator` → `~/.pi/config.yaml` |
| AtomCode 生成器 | ✅ | `AtomCodeGenerator` → `~/.atomcode/settings.json` |
| Kilo 生成器 | ✅ | `KiloGenerator` → `~/.kilo/config.json` |
| 备份机制 | ✅ | 覆写前备份到 `~/.ntd/profile_backups/`，保留最近 10 份 |
| Profile CRUD API | ✅ | 6 个 RESTful 端点 |
| 切换 apply API | ✅ | 写配置文件 + 更新 current_profile 标记 |
| 前端 Profile 管理面板 | ✅ | 设置页「API Key」Tab（列表/创建/编辑/删除/切换） |
| API Key 遮盖显示 | ✅ | 前端 maskKey 只显示首尾各 4 位 |

## 3.2 未完成（后续版本）

- 其余 9 个执行器的生成器（CodeBuddy、Opencode、Kimi、Mimo、Zhanlu、Hermes、Codex、CodeWhale、MobileCoder）
- CLI 命令 `ntd exec profile list|switch|apply`
- API Key 加密存储
- Profile 导入/导出

# 4. 新增/修改文件清单

## 新增文件

| 文件 | 说明 |
|------|------|
| `backend/src/profiles.rs` | 数据结构 + API DTO + 加载/保存 + GeneratorRegistry |
| `backend/src/profiles/generators.rs` | 4 个内置执行器的配置生成器 |
| `backend/src/handlers/profiles.rs` | Profile CRUD + apply API handlers |
| `frontend/src/components/settings/ProfilesPanel.tsx` | 前端 API Key 管理面板 |

## 修改文件

| 文件 | 修改内容 |
|------|---------|
| `backend/src/lib.rs` | 注册 `pub mod profiles` |
| `backend/src/handlers/mod.rs` | 注册 `profiles::profile_routes()`；更新路由计数测试 |
| `frontend/src/components/SettingsPage.tsx` | 增加 `KeyOutlined` 图标导入、`ProfilesPanel` 导入、新增 Tab |

# 5. API 定义

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/v1/profiles` | 获取所有 profile 摘要列表 |
| GET | `/api/v1/profiles/current` | 获取当前 profile 详情 |
| POST | `/api/v1/profiles` | 创建新 profile |
| PUT | `/api/v1/profiles/{name}` | 更新 profile |
| DELETE | `/api/v1/profiles/{name}` | 删除 profile（当前激活不可删） |
| POST | `/api/v1/profiles/{name}/apply` | 切换并写配置文件 |

# 6. 测试结果

```
cargo clippy --all-targets -- -D warnings  →  ✅ 零告警
cargo test                                 →  ✅ 1295 passed, 0 failed
npx tsc --noEmit                          →  ✅ 零错误
```

# 7. 已知问题与后续改进

## 当前局限

1. **API Key 明文存储**：`profiles.yaml` 中 API Key 以明文保存，依赖文件权限保护
2. **仅覆盖 4/13 执行器**：其余执行器的生成器尚未实现
3. **无 CLI 命令**：当前仅能通过 Web UI 和 REST API 操作

## 后续改进方向

1. **加密存储**：引入 AES 加密 API Key，密钥从系统 keychain 派生
2. **更多执行器**：逐步添加 CodeBuddy、Opencode、Kimi、Mimo 等生成器
3. **CLI 命令**：`ntd exec profile list|switch|apply`
4. **导入/导出**：支持 Profile 的 JSON/YAML 文件导入导出
