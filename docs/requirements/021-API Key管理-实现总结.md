# 0. 文件修改记录表

| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| AI | 2026-07-23 | 初始版本 |

# 1. 实现概述

为 ntd 实现了 **API Key 统一管理系统**，支持各 AI 代码执行器的凭据集中管理、按需写入配置文件。

核心思路：每个 API Key 一张卡片，应用时通过三步流程（选择执行器 → 预览内容 → 确认写入）将凭据和模型灌入对应执行器的原生配置文件。

# 2. 对应需求

- **需求文档**：`docs/requirements/021-API Key管理-需求.md`
- **设计文档**：`docs/设计文档/021-API Key管理-设计.md`
- **测试文档**：`docs/testing/021-API Key管理-测试.md`

# 3. 实现功能清单

## 3.1 已完成

| 功能 | 状态 | 说明 |
|------|------|------|
| Provider 数据结构 | ✅ | name / api_key / base_url / protocol / models（每模型可选 supports_1m_context） |
| profiles.yaml 加载/保存 | ✅ | 原子写（temp + rename） |
| 4 个生成器实现 | ✅ | ClaudeCodeGenerator / PiGenerator / AtomCodeGenerator / KiloGenerator |
| 协议自适应 | ✅ | Anthropic 协议 → env 块（Claude Code）/ pi 用 anthropic-messages / 其他用 OpenAI-compatible |
| 1M 上下文属性 | ✅ | 模型级别（`supports_1m_context` per model） |
| 备份机制（同目录 .bak-） | ✅ | 保留最近 5 份 |
| AtomCode 去重 | ✅ | 先删旧 `[providers.ntd-profile]` 段，重复 apply 不重复 |
| PI 适配真实格式 | ✅ | 写入 `~/.pi/agent/models.json` + `~/.pi/agent/settings.json` |
| Provider CRUD API | ✅ | 6 个端点：list / create / read / update / delete / supported-executors |
| Preview API | ✅ | POST /providers/{name}/preview 返回路径 + 内容 |
| Apply API | ✅ | POST /providers/{name}/apply 写入 |
| 执行器配置定义 API | ✅ | GET /providers/supported-executors 返回 13 个执行器（路径 + 是否有生成器） |
| 卡片面板 UI | ✅ | 协议标签、模型列表、1M 标记、操作按钮 |
| 三步应用弹窗 | ✅ | 选执行器 + 模型 → Tab 预览 → 写入结果 |
| 移动端适配 | ✅ | 单列卡片、全屏弹窗 |
| 暗色主题适配 | ✅ | 代码预览块背景/文字颜色自适应 |
| 协议下拉框 | ✅ | OpenAI 兼容 / Anthropic 原生 二选一（必填） |
| 1300+ 单元测试通过 | ✅ | cargo test 全绿 |
| 编译零告警 | ✅ | cargo clippy -- -D warnings |
| API Key 导入/导出 | ✅ | YAML 格式导出文件，merge/replace 策略导入 |

## 3.2 未完成（按设计意图不在本期）

- 其他 9 个执行器的生成器（CodeBuddy/Opencode/Kimi/Mimo/Zhanlu/Hermes/Codex/CodeWhale/MobileCoder）
- API Key 加密存储
- Profile 配置模板批量切换（已被用户移除该层）
- 配置文件深度合并（保留用户原有字段）
- 云同步

# 4. 新增/修改文件清单

## 新增文件

| 文件 | 说明 |
|------|------|
| `backend/src/profiles.rs` | Provider 数据结构 + 加载/保存 |
| `backend/src/profiles/generators.rs` | 4 个生成器 + 备份逻辑 + all_executor_configs |
| `backend/src/handlers/profiles.rs` | API 端点 |
| `frontend/src/components/settings/ProfilesPanel.tsx` | 卡片面板 + 三步应用弹窗 |

## 修改文件

| 文件 | 修改内容 |
|------|---------|
| `backend/src/lib.rs` | 注册 `pub mod profiles` |
| `backend/src/handlers/mod.rs` | 注册路由 + 路由计数测试 |
| `frontend/src/components/SettingsPage.tsx` | 设置页新增「API Key」Tab |

# 5. API 定义

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/v1/providers` | 供应商列表 |
| GET | `/api/v1/providers/supported-executors` | 执行器配置定义 |
| GET | `/api/v1/providers/{name}` | 详情（含 api_key） |
| POST | `/api/v1/providers` | 创建 |
| PUT | `/api/v1/providers/{name}` | 更新 |
| DELETE | `/api/v1/providers/{name}` | 删除 |
| POST | `/api/v1/providers/{name}/preview` | 预览（不写盘） |
| POST | `/api/v1/providers/{name}/apply` | 应用（写盘 + 备份） |

# 6. 迭代历程

按时间顺序的关键迭代（反映了用户反馈驱动的设计调整）：

1. **初始设计**：每个 Profile 包含供应商列表（用户偏好"供应商池 = API Key 库"模型）
2. **架构大改**：用户反馈"Profile 是为了批量给执行器一次性设置大模型吗？" → 改为两层（供应商池 + Profile 引用）
3. **第二次大改**：用户反馈"为什么要有 Profile？" → 完全砍掉 Profile UI
4. **第三次改**：用户说"应该是 API Key 一张卡片 + 应用按钮" → 改为卡片式 + 应用弹窗
5. **第四次改**：用户要求"按执行器生成预览，一个 Tab 一个执行器" → 三步流程（选 → 预览 → 确认）
6. **第五次改**：发现 PI 配置路径错误（写到 `~/.pi/config.yaml`，实际是 `~/.pi/agent/models.json`） → 重写 PiGenerator
7. **第六次改**：用户要求"提取执行器配置定义到统一 API，不要写死路径" → 新增 supported-executors API
8. **第七次改**：用户要求"按执行器选择模型" → ApplyProviderRequest 改为 `executor_models: HashMap`
9. **细节调整**：
   - 支持 1M 上下文 → 移到模型级
   - 协议下拉框化
   - 暗色主题适配
   - 备份改用 `{原文件名}.bak-{时间戳}` 同目录格式
   - AtomCode 修复重复 apply
10. **UI 修复**：点击全选 bug 修复（改用原生 input + 独立状态）

# 7. 测试结果

```
cargo clippy --all-targets -- -D warnings  ✅ 零告警
cargo test                                 ✅ 1294+ passed
npx tsc --noEmit                          ✅ 零错误
```

# 8. 已知问题与后续改进

## 当前局限

1. **完整覆写**：Claude Code / Kilo 完整覆写配置，丢失用户已有其他字段
2. **provider 替换**：PI 替换整个 provider 条目，丢失用户对该 provider 的自定义字段（如 maxTokens）
3. **覆盖范围**：仅 4 个执行器支持生成器（首批）

## 后续改进方向

1. 实现其余 9 个执行器的生成器
2. 引入 AES 加密 API Key
3. 配置文件深度合并（保留用户原始字段，仅覆盖 ntd 管理的键）
4. 接入 Git 同步支持跨设备
5. 检测执行器配置文件的"用户字段"，分离 ntd 管理和用户自定义部分
6. 多 Profile 概念可选加回（如团队需要批量切换场景时）
