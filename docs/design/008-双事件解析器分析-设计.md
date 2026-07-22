# 双重事件解析系统分析报告

## 1. 背景

项目中存在两套并行的事件解析系统：

| 维度 | 旧系统（adapters/） | 新系统（execution_events/） |
|------|---------------------|---------------------------|
| 核心 trait | `CodeExecutor` | `EventExtractor` |
| 解析方法 | `parse_output_line` / `parse_stderr_line` | `extract` / `extract_stderr` |
| 输出类型 | `ParsedLogEntry`（string log_type） | `ExecutionEvent`（强类型枚举） |
| 管道 | 无（逐行调用，由外部编排） | `EventPipeline`（内置元数据累积 + finalize） |
| 元数据 | 分散在各 executor 的 `Arc<Mutex<...>>` 中 | 集中在 `ExecutionMetadata` |
| 实现文件数 | 7 个 `*_event.rs` + 13 个 executor `.rs` | 14 个 `impls/*.rs` |

**核心问题**：两套系统在 `log_capture.rs` 中同时运行——EventPipeline 优先，失败后回退到旧的 `parse_output_line`。职责重叠，且旧系统尚未彻底清除。

---

## 2. 两套系统职责对比

### 2.1 事件解析（完全重叠）

旧系统每个 executor 的 `parse_output_line` 实现的逻辑，与新系统对应 Extractor 的 `extract` 实现**完全等价**。

以 MiMo 为例对比：

| 解析目标 | 旧系统 `MimoExecutor::parse_output_line` | 新系统 `MimoExtractor::extract` |
|----------|------------------------------------------|--------------------------------|
| `step_start` | `entry("step_start", "Step started")` | `ExecutionEvent::StepStart { .. }` |
| `text` | `text_entry(text)` | `ExecutionEvent::Assistant { content, .. }` |
| `reasoning` | `entry("thinking", trimmed)` | `ExecutionEvent::Thinking { .. }` |
| `tool_use` | `entry_with_optional_tool("tool", ...)` | `ExecutionEvent::ToolCall { .. }` |
| `step_finish` | `entry_with_usage("step_finish", ...)` | `ExecutionEvent::Tokens + StepFinish` |

两套解析器处理**同一行 JSONL 输出**，结果语义完全相同。

### 2.2 旧系统独有的能力（新系统尚未覆盖）

`CodeExecutor` trait 中有 9 个方法**不属于事件解析**，但目前只有旧系统提供：

| 方法 | 职责 | 当前调用方 | 新系统是否需要 |
|------|------|-----------|---------------|
| `command_args` | 构建 CLI 命令参数 | `pre_spawn.rs`, `message_debounce.rs` | **需要**，但不应属于"解析器" |
| `command_args_with_session` | 带 session 的命令参数 | `pre_spawn.rs` | **需要**，同上 |
| `supports_resume` | 是否支持续接对话 | `handlers/execution.rs` | **需要** |
| `extract_session_id` | 从行中提取 session_id | `log_capture.rs:433` | **已由 EventPipeline 覆盖**（metadata.session_id） |
| `check_success` | 退出码判定成功 | `spawn_lifecycle.rs:614` | **需要** |
| `get_final_result` | 从日志提取最终结论 | `completion.rs`（已被 pipeline.finalize() 替代） | **已由 EventPipeline 覆盖** |
| `get_model` | 获取模型名称 | `completion.rs`（已被 pipeline 元数据替代） | **已由 EventPipeline 覆盖** |
| `post_execution_todo_progress` | 后置进度钩子 | `completion.rs:192` | **需要**（仅 hermes 使用） |
| `get_tool_calls_count` | 工具调用次数 | `completion.rs`（已被 stats 替代） | **已由 EventPipeline 覆盖** |
| `stdin_payload` | stdin 预写内容 | `spawn_lifecycle.rs:169` | **需要**（仅 pi 使用） |

### 2.3 数据类型重复

`*_event.rs` 文件定义的事件数据结构与新系统高度重复：

| 旧结构体 | 新对应 | 是否本质相同 |
|----------|--------|------------|
| `MimoEvent/MimoPart/MimoTokens` | `serde_json::Value` + 手动解析 | 完全等价 |
| `OpencodeAgentEvent/OpencodeAgentPart` | `OpencodeAgentEvent`（复用旧类型） | **新系统直接 import 旧类型** |
| `KiloAgentEvent/KiloAgentPart` | `KiloAgentEvent`（复用旧类型） | **新系统直接 import 旧类型** |
| `AgentEvent/AgentPart/AgentTokens` | `serde_json::Value` + 手动解析 | 完全等价 |
| `ZhanluAgentEvent` | `serde_json::Value` + 手动解析 | 完全等价 |
| `MobilecoderAgentEvent` | `serde_json::Value` + 手动解析 | 完全等价 |
| `PiEvent/PiContentBlock/PiUsage` | `serde_json::Value` + 手动解析 | 完全等价 |
| `ClaudeMessage/ClaudeContentBlock` | `ClaudeMessage`（复用旧类型） | **新系统直接 import 旧类型** |

---

## 3. 当前共存的调用链路

### 3.1 stdout 解析流程（`log_capture.rs:spawn_stdout_reader`）

```
stdout 行
    │
    ├─ EventPipeline.feed(line)          ← 新系统优先
    │      ↓
    │  产生 ExecutionEvent?
    │      ├─ YES → 转换 ParsedLogEntry，emit Output，写 DB
    │      └─ NO  ↓
    │
    ├─ executor.extract_session_id(line) ← 旧系统 fallback
    │
    └─ executor.parse_output_line(line)  ← 旧系统 fallback
           ↓
       产生 ParsedLogEntry?
           ├─ YES → emit Output，写 DB
           └─ NO  → 跳过
```

### 3.2 stderr 解析流程（`log_capture.rs:spawn_stderr_reader`）

```
stderr 行
    │
    ├─ EventPipeline.feed_stderr(line)   ← 新系统优先
    │      ↓
    │  产生 ExecutionEvent?
    │      ├─ YES → 转换 ParsedLogEntry，emit Output，写 DB
    │      └─ NO  ↓
    │
    └─ executor.parse_stderr_line(line)  ← 旧系统 fallback
```

### 3.3 执行生命周期中的旧系统调用

| 阶段 | 旧系统调用 | 新系统是否可替代 |
|------|-----------|----------------|
| 命令构建 | `executor.command_args()` | 需要新 trait 覆盖 |
| Session 管理 | `executor.command_args_with_session()` | 需要新 trait 覆盖 |
| Resume 判定 | `executor.supports_resume()` | 需要新 trait 覆盖 |
| Session ID 提取 | `executor.extract_session_id()` | **已被 Pipeline metadata 覆盖** |
| 退出码判定 | `executor.check_success()` | 需要新 trait 覆盖 |
| 最终结论 | `executor.get_final_result()` | **已被 pipeline.finalize() 覆盖** |
| 模型提取 | `executor.get_model()` | **已被 Pipeline metadata 覆盖** |
| 进度钩子 | `executor.post_execution_todo_progress()` | 需要新 trait 覆盖 |
| 工具调用统计 | `executor.get_tool_calls_count()` | **已被 stats 计算覆盖** |
| stdin 预写 | `executor.stdin_payload()` | 需要新 trait 覆盖 |

---

## 4. 新系统对旧事件类型文件的依赖

新系统 Extractor 中有 3 个**直接 import 旧 `*_event.rs` 类型**：

| 新 Extractor | 依赖的旧类型 | 位置 |
|-------------|-------------|------|
| `ClaudeCodeExtractor` | `ClaudeMessage`, `ClaudeContentBlock` | `adapters/claude_protocol.rs` |
| `OpencodeExtractor` | `OpencodeAgentEvent`, `OpencodeAgentToolInput` | `adapters/opencode_event.rs` |
| `KiloExtractor` | `KiloAgentEvent` | `adapters/kilo_event.rs` |

这意味着：**删除旧 `*_event.rs` 前，需要先把这些类型迁移到新系统或独立模块**。

---

## 5. 清除旧解析系统的可行性分析

### 5.1 可以安全清除的部分

| 文件 | 原因 |
|------|------|
| `adapters/agent_event.rs` | 未被任何 executor 实际使用（仅定义了 `AgentEvent` 类型，无调用） |
| `adapters/mimo_event.rs` | `MimoExtractor` 已用 `serde_json::Value` 手动解析替代，不再依赖此类型 |
| `adapters/zhanlu_event.rs` | `ZhanluExtractor` 已用 `serde_json::Value` 手动解析替代 |
| `adapters/mobilecoder_event.rs` | `MobilecoderExtractor` 已用 `serde_json::Value` 手动解析替代 |
| `adapters/pi_event.rs` | `PiExtractor` 已用 `serde_json::Value` 手动解析替代 |
| 各 executor 的 `parse_output_line` 实现 | EventPipeline 已完全覆盖解析逻辑 |
| 各 executor 的 `parse_stderr_line` 实现 | EventPipeline 已完全覆盖 |
| `executor.get_final_result()` | `pipeline.finalize()` + `completion.rs:get_final_result_from_logs` 已替代 |
| `executor.get_model()` | `pipeline.metadata().model` + `completion.rs:get_model_from_logs` 已替代 |
| `executor.extract_session_id()` | `pipeline.metadata().session_id` 已替代 |
| `executor.get_tool_calls_count()` | `log_capture.rs:extract_execution_stats` 已替代 |

### 5.2 需要迁移后才能清除的部分

| 文件/方法 | 迁移目标 | 难度 |
|-----------|---------|------|
| `adapters/opencode_event.rs` | 迁移到 `execution_events/types/` 或 inline 到 `OpencodeExtractor` | 中 |
| `adapters/kilo_event.rs` | 迁移到 `execution_events/types/` 或 inline 到 `KiloExtractor` | 中 |
| `adapters/claude_protocol.rs` | 迁移到 `execution_events/types/claude_protocol.rs` | 低（已无其他依赖） |
| `executor.command_args()` | 新增 trait 方法（如 `ExecutorConfig` trait） | 低 |
| `executor.command_args_with_session()` | 同上 | 低 |
| `executor.supports_resume()` | 同上 | 低 |
| `executor.check_success()` | 同上 | 低 |
| `executor.post_execution_todo_progress()` | 同上 | 低 |
| `executor.stdin_payload()` | 同上 | 低 |

### 5.3 不可清除（需保留在 adapters/ 的部分）

| 模块 | 原因 |
|------|------|
| `adapters/mod.rs` 中的 `ExecutorDef` / `EXECUTORS` / `find_executor` / `parse_executor_type` | 全局 executor 注册与查找，被 `handlers/`、`db/executor_config.rs`、`pre_spawn.rs` 广泛使用 |
| `adapters/mod.rs` 中的 `ExecutorRegistry` | executor 实例管理，被 `service_context.rs`、`handlers/mod.rs`、`loop_runner.rs` 使用 |
| `adapters/mod.rs` 中的 `RESUMABLE_EXECUTORS` / `DEFAULT_EXECUTOR` | 常量定义，被多处引用 |
| `adapters/mod.rs` 中的 `strip_think_tags` / `default_final_result_with_think_stripping` | 工具函数，可能仍有调用方 |
| `adapters/mod.rs` 中的 `BaseExecutor` | 组合模式的基础设施（但可随迁移逐步移除） |

---

## 6. 推荐清除步骤（分 4 个阶段）

### Phase 1：解除新系统对旧 `*_event.rs` 的依赖（低风险）

**目标**：让新系统不再 import 任何旧 `*_event.rs` 类型。

1. 把 `ClaudeMessage` / `ClaudeContentBlock` 从 `adapters/claude_protocol.rs` 迁移到 `execution_events/types/claude.rs`
2. 把 `OpencodeAgentEvent` 相关类型从 `adapters/opencode_event.rs` 迁移到 `execution_events/types/opencode.rs`
3. 把 `KiloAgentEvent` 相关类型从 `adapters/kilo_event.rs` 迁移到 `execution_events/types/kilo.rs`
4. 在原 `adapters/*.rs` 中保留 re-export（兼容旧代码），或直接删除旧文件

**风险**：低。只是文件移动，逻辑不变。

### Phase 2：删除未被使用的旧事件类型文件（零风险）

**目标**：清理完全未被 import 的旧文件。

可直接删除：
- `adapters/agent_event.rs`
- `adapters/mimo_event.rs`
- `adapters/zhanlu_event.rs`
- `adapters/mobilecoder_event.rs`
- `adapters/pi_event.rs`

同时删除 `adapters/mod.rs` 中对应的 `pub mod` 声明。

**风险**：零。编译通过即证明安全。

### Phase 3：移除 log_capture.rs 中的旧解析 fallback（中风险）

**目标**：让 `spawn_stdout_reader` 和 `spawn_stderr_reader` 不再调用旧 `parse_output_line` / `parse_stderr_line`。

操作：
1. 在 `spawn_stdout_reader` 中，删除 `executor_clone.parse_output_line(&line)` 分支
2. 在 `spawn_stderr_reader` 中，删除 `executor.parse_stderr_line(&line)` 分支
3. 保留 `executor.extract_session_id()` 作为**临时** fallback（Phase 4 再移除）
4. 确保所有 13 个 executor 的 EventExtractor 实现都已覆盖其全部事件类型

**风险**：中。如果某个 executor 的 EventExtractor 实现不完整，该 executor 的日志会丢失。需要逐一验证每个 executor 的 Extractor 覆盖率。

**验证方法**：对每个 executor，构造其典型输出样例，分别过新旧解析器，对比 `ParsedLogEntry` 输出是否一致。

### Phase 4：提取 executor 配置能力到新 trait + 删除旧 CodeExecutor（高风险）

**目标**：将 `command_args`、`supports_resume`、`check_success` 等非解析能力提取到新 trait，然后删除旧 `CodeExecutor`。

操作：
1. 在 `execution_events/` 或 `adapters/` 中定义新 trait `ExecutorConfig`（或 `ExecutorBehavior`），包含：
   - `fn command_args(&self, message: &str) -> Vec<String>`
   - `fn command_args_with_session(&self, message: &str, session_id: Option<&str>, is_resume: bool) -> Vec<String>`
   - `fn supports_resume(&self) -> bool`
   - `fn check_success(&self, exit_code: i32) -> bool`
   - `fn stdin_payload(&self) -> Option<String>`
   - `fn post_execution_todo_progress(&self) -> Option<Vec<TodoItem>>`
2. 让每个 executor 实现新 trait
3. 修改 `pre_spawn.rs`、`spawn_lifecycle.rs`、`completion.rs`、`message_debounce.rs` 等调用方，改为使用新 trait
4. 删除旧 `CodeExecutor` trait 和所有旧 executor 实现文件（`adapters/claude_code.rs`、`adapters/mimo.rs` 等 13 个）
5. 删除 `adapters/mod.rs` 中的 `BaseExecutor`、`ExecutorRegistry`（或将其迁移到新位置）

**风险**：高。涉及 13 个 executor 实现 + 6+ 个调用方的重构，需要全面回归测试。

---

## 7. 风险清单

| 风险 | 影响 | 缓解措施 |
|------|------|---------|
| EventExtractor 实现不完整导致日志丢失 | 用户看不到部分执行过程 | Phase 3 前对每个 executor 做输入样例对比测试 |
| `check_success` 语义差异 | mimo/opencode/zhanlu/kilo 的"非零退出码但成功"语义丢失 | 新 trait 中保留 `check_success` 方法 |
| `post_execution_todo_progress` 丢失 | hermes 的进度钩子失效 | 在新 trait 中保留此方法 |
| `stdin_payload` 丢失 | pi 的自动确认行为失效 | 在新 trait 中保留此方法 |
| 旧 `*_event.rs` 类型被新系统依赖 | 删除旧文件导致编译失败 | Phase 1 先解除依赖 |
| `ExecutorRegistry` 位置变动 | `handlers/`、`service_context.rs` 等大量调用方受影响 | Phase 4 中保留 registry 在原位或做好 re-export |
| `message_debounce.rs` 使用旧 executor 的 `command_args` | 改动影响 debounce 逻辑 | Phase 4 中统一迁移到新 trait |

---

## 8. 工作量估算

| 阶段 | 预估工作量 | 前置条件 |
|------|-----------|---------|
| Phase 1 | 1-2 小时 | 无 |
| Phase 2 | 10 分钟 | Phase 1 |
| Phase 3 | 2-3 小时（含验证） | Phase 1 |
| Phase 4 | 4-6 小时（含回归测试） | Phase 3 |

**总计**：约 1-2 个工作日。

---

## 9. 结论

1. **可以清除旧解析系统**，但需要分阶段进行，不能一次性删除。
2. **Phase 1-2（低风险）** 可以立即执行，解除新系统对旧类型的依赖 + 清理无用文件。
3. **Phase 3（中风险）** 是关键转折点——移除 `parse_output_line` fallback 后，新系统成为唯一的解析路径。需要确保所有 executor 的 Extractor 实现完备。
4. **Phase 4（高风险）** 是最彻底的清理，将 executor 的配置/行为能力从旧 trait 迁移到新架构。
5. **建议优先级**：Phase 1 > Phase 2 > Phase 3 > Phase 4。Phase 1-2 可以在当前迭代完成，Phase 3-4 可以作为后续技术债务清理任务。
