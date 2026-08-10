# 093-kilo系执行器提炼基类-设计

| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| AI (Pi) | 2026-08-08 | 初始版本 |
| AI (zhanlu) | 2026-08-10 | 按 CodeRabbit 评审修正：差异方法口径（5 行为查询+1 类型映射）、方法名与代码对齐、验证方案措辞（去掉"全绿"，写明 git_sync 环境失败） |

> 093 优化扫描专项 B1（重构批次 1）。源自 code-refactor/design-patterns skill 扫描诊断第 1 条：
> **Copy-Paste Programming 反模式**——4 个执行器适配器最高 83% 逐字相同。

## 1. 背景与证据

### 1.1 复制粘贴实证（diff 实测）

| 文件对 | 差异行/总行 | 相同度 |
|--------|-----------|--------|
| `kilo.rs` vs `opencode.rs` | 76/438 | 83% |
| `opencode.rs` vs `zhanlu.rs` | 76/431 | 82% |
| `kilo_event.rs` vs `opencode_event.rs` | 仅注释 | ~100% |
| `opencode_event.rs` vs `zhanlu_event.rs` | 仅注释 | ~100% |

事件结构体文件头部注释自己写明：「只把类型名从 Opencode 前缀重命名，serde 结构和字段映射保持完全一致」。

代价已经兑现过两次：NTD-012 修复时同一 `on_pipeline_event` 实现插了 4 遍；任何协议调整都要 4 处同步（Shotgun Surgery）。

### 1.2 逐行为差异矩阵（完整盘点，无遗漏）

| 行为点 | kilo | opencode | zhanlu | mimo |
|--------|------|----------|--------|------|
| 事件名风格 | `step_start`\|`step-start` 双兼容 | 同 kilo | 同 kilo | 仅下划线式 |
| `reasoning` 事件 | 无 | 无 | 无 | 有（→thinking，截 500 字符） |
| tool_use 的 JSON 载荷 | `input.to_full_json()` | 同 kilo | 同 kilo | 序列化整个 state（前端依赖 `state.status`） |
| resume 无 session_id 时 | 不加参数 | 同 kilo | 同 kilo | 降级 `-c`（续接最近会话） |
| `get_model` | `base.model` | 同 kilo | 同 kilo | 恒 `None` |
| part 字段 serde | snake_case | 同 kilo | 同 kilo | camelCase（`callID`/`messageID`/`sessionID`）+ `snapshot` 字段 |
| `to_full_json` | extra 同名键可覆盖 command/description | 同 kilo | 同 kilo | extra 同名键跳过（防覆盖核心语义） |

## 2. 设计

模式选型：**Template Method 的 Rust 组合版**——单一 `StepProtocolExecutor` 结构体 +
`StepProtocolFlavor` 枚举承载运行时行为差异（数据化配置，替代 4 份复制类）。

### C1：统一事件模型 `adapters/step_event.rs`（新）

一份 canonical 结构体（`StepAgentEvent`/`StepAgentPart`/`StepAgentToolState`/`StepAgentToolInput`/`StepAgentTokens`/`StepAgentCacheTokens`）：

- mimo 的 camelCase 用 `#[serde(alias = "callID")]` 等 **alias 兼容**（反序列化接受两种键名，序列化恒输出 snake_case——与 mimo 现状一致）；
- `snapshot` 字段并入（`Option`，非 mimo 恒 None，无行为影响）；
- `ToolState`/`ToolInput` 补 `Serialize` derive（mimo 的 state_json 序列化需要；kilo 系不用，无害）；
- `to_full_json` 采用 mimo 的防覆盖版本（**有意的语义对齐**：extra 同名键不再覆盖 command/description，比 kilo 系现状更安全；该场景仅在 extra 恰好含同名键时触发，概率极低，记录为行为微差异）。

### C2：统一执行器 `adapters/step_protocol.rs`（新）

```rust
pub struct StepProtocolExecutor {
    base: BaseExecutor,
    has_successful_finish: Arc<Mutex<bool>>,
    session_id: Arc<Mutex<Option<String>>>,
    flavor: StepProtocolFlavor,   // 5 个行为差异点的唯一载体
}
```

- 4 个命名构造：`StepProtocolExecutor::{kilo, opencode, zhanlu, mimo}(path)`；
- flavor 上的查询方法共 6 个：5 个协议行为查询 `accepts_hyphenated_events()` / `has_reasoning_event()` / `serializes_full_tool_state()` / `resume_falls_back_to_dash_c()` / `reports_model()` + 1 个类型映射 `executor_type()`——**flavor 控制的运行时行为差异**全部集中于此；序列化层差异（camelCase alias、`snapshot`、`to_full_json` 防覆盖）由 C1 的 `step_event.rs` 承载，不在本枚举职责内；
- 共享逻辑一份：`handle_step_start/tool_use/text/step_finish/reasoning`、`parse_output_line`、`extract_session_id`、`on_pipeline_event`、`check_success`、`command_args(_with_session)`；
- 旧 4 文件（kilo.rs/opencode.rs/zhanlu.rs/mimo.rs，共 1840 行）**删除**，测试并集迁移到本文件。

### C3：事件模块壳（保持引用路径）

`kilo_event.rs`/`opencode_event.rs`/`zhanlu_event.rs`/`mimo_event.rs` 各缩为类型别名再导出（`pub use super::step_event::{StepAgentEvent as KiloAgentEvent, ...}`）——`execution_events/impls/{kilo,opencode}.rs` 等现有引用方零改动。

### C4：调用点适配

- `adapters/mod.rs`：4 个 `pub mod` 声明换为 `step_event`/`step_protocol`；注册处 `KiloExecutor::new(...)` → `StepProtocolExecutor::kilo(...)` 等 4 处；
- `executor_service/log_capture.rs:774`（NTD-012 回归测试）构造器改名。

## 3. 影响模块

| 文件 | 变化 |
|------|------|
| `adapters/step_event.rs` | 新增（~140 行） |
| `adapters/step_protocol.rs` | 新增（~450 行实现 + ~400 行统一测试） |
| `adapters/{kilo,opencode,zhanlu,mimo}.rs` | 删除（1840 行） |
| `adapters/{kilo,opencode,zhanlu,mimo}_event.rs` | 缩为别名壳（~100 行 → 各 ~15 行） |
| `adapters/mod.rs`、`log_capture.rs` | 调用点改名 |

净减约 1800 行；行为矩阵逐格保持不变（除 §2-C1 声明的 to_full_json 防覆盖对齐）。

## 4. 验证方案

1. 统一测试套件覆盖原 4 套测试的并集：各 flavor 的事件解析（下划线/连字符）、reasoning（mimo）、tool_use 两种 JSON 载荷形态、check_success 三态（0/无 finish 非零/有 finish 非零）、session_id 缓存、command_args（-m 注入、mimo `-c` 降级）、get_model 差异。
2. NTD-012 回归测试（log_capture）改编后构造器继续通过。
3. `cargo clippy --all-targets -- -D warnings` 零告警；`cargo test` 预期 1627 通过、1 个预存量环境失败（`git_sync` 测试：本机 git 版本过老不支持 `init -b`，与本次改动无关，main 上同样失败）。
4. 反证：diff 级核对「统一实现 vs 各旧实现」的行为矩阵逐格一致（设计 §1.2 表即核对清单）。

## 5. 安全反思

- 纯内部重构，无接口/schema/协议变化；执行器进程间通信的 JSONL 解析行为逐格保持；
- serde alias 只放宽反序列化输入面，不改变输出；
- 注册表键（"kilo"/"mimo"/...）不变，用户配置无感。
