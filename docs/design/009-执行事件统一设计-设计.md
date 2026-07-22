# 执行反馈统一层设计方案

## 1. 背景与目标

### 1.1 问题现状

当前系统存在以下问题：

1. **各执行器解析逻辑分散**：13+ 个执行器（claude_code, kilo, opencode 等）各自解析输出，log_type 命名不一致
   - claude_code: `tool_use`, `tool_result`
   - kilo: `tool_call`, `tool_result`
   - 其他执行器各有一套命名

2. **信息提取分散**：每个执行器都要重复实现：
   - session_id 提取
   - token usage 提取
   - tool_name / tool_input_json 提取
   - model 提取

3. **数据流不统一**：
   - 前端 WebSocket 需要结构化日志
   - 飞书推送需要格式化文本
   - 数据库落库需要兼容格式
   - 三方各自处理，难以扩展

4. **ParsedLogEntry 承担过多职责**：
   - 既是原始日志容器
   - 又要塞入结构化信息（通过 metadata JSON）
   - log_type 是字符串，IDE 无法进行类型检查

### 1.2 目标

1. **统一事件抽象**：用强类型 `ExecutionEvent` 枚举替代字符串化的 `log_type`
2. **统一元数据管理**：用 `ExecutionMetadata` 集中管理 session_id、token、cost 等
3. **统一数据流**：执行器 → 事件管道 → 前端/飞书/数据库
4. **向后兼容**：保持数据库表结构不变，log_type 值统一

---

## 2. 架构设计

### 2.1 目录结构

```
backend/src/
├── execution_events/              # 新建独立模块
│   ├── mod.rs                    # 模块入口，导出公共 API
│   ├── event.rs                  # ExecutionEvent 枚举定义
│   ├── metadata.rs               # ExecutionMetadata 元数据结构
│   ├── extractor.rs              # EventExtractor trait
│   ├── pipeline.rs               # EventPipeline 事件处理管道
│   ├── db_adapter.rs             # 数据库适配：事件 → execution_logs 映射
│   ├── ws_adapter.rs             # WebSocket 适配：事件 → WS 消息
│   ├── feishu_adapter.rs         # 飞书适配：事件 → 飞书卡片
│   └── impls/                    # 各执行器的事件提取实现
│       ├── mod.rs
│       ├── claude_code.rs
│       ├── opencode.rs
│       ├── kilo.rs
│       ├── mimo.rs
│       └── ... (其他执行器)
```

### 2.2 核心类型

#### 2.2.1 ExecutionEvent 枚举

```rust
// execution_events/event.rs

/// 统一的事件类型枚举，完全替代 ParsedLogEntry
///
/// # 设计原则
/// - 使用 #[serde(tag = "type")] 实现 JSON 中的 "type" 字段自动序列化
/// - 每个变体都是独立的语义单元
/// - 向后兼容：最终会映射到 execution_logs.log_type 的已知值
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum ExecutionEvent {
    // ── 消息类型 ────────────────────────────────────────
    /// 助手消息（可能是包含 thinking 的复合消息）
    Assistant {
        content: String,
        thinking: Option<String>,
        message_id: Option<String>,
    },
    /// 思考过程（从 <thinking> 标签或 thinking 事件提取）
    Thinking { content: String },
    /// 工具调用发起
    ToolCall {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    /// 工具调用结果
    ToolResult {
        call_id: String,
        output: String,
        is_error: bool,
    },
    /// 最终结果/总结
    Result { summary: String },
    /// 用户消息
    User { content: String },
    /// 系统消息
    System { message: String },
    /// 普通信息/日志
    Info { message: String },
    /// 错误消息
    Error { message: String },

    // ── 执行阶段 ───────────────────────────────────────────
    /// 执行步骤开始
    StepStart { name: String, index: u32 },
    /// 执行步骤完成
    StepFinish { name: String, index: u32 },

    // ── 元数据事件 ─────────────────────────────────────────
    /// Token 统计（可能累积或单次报告）
    Tokens {
        input: u64,
        output: u64,
        cache_read: Option<u64>,
        cache_write: Option<u64>,
    },
    /// 会话开始
    SessionStart { session_id: String },
    /// 会话结束
    SessionEnd { session_id: String },
    /// 模型切换
    ModelSwitch { model: String },
    /// 成本报告
    Cost { cost_usd: f64 },
    /// 耗时报告
    Duration { duration_ms: u64 },
    /// 进度更新
    Progress { percent: u8, message: Option<String> },
}

impl ExecutionEvent {
    /// 转换为数据库兼容的 log_type 字符串
    pub fn to_log_type(&self) -> &'static str {
        match self {
            ExecutionEvent::Assistant { .. } => "assistant",
            ExecutionEvent::Thinking { .. } => "thinking",
            ExecutionEvent::ToolCall { .. } => "tool_call",
            ExecutionEvent::ToolResult { .. } => "tool_result",
            ExecutionEvent::Result { .. } => "result",
            ExecutionEvent::User { .. } => "user",
            ExecutionEvent::System { .. } => "system",
            ExecutionEvent::Info { .. } => "info",
            ExecutionEvent::Error { .. } => "error",
            ExecutionEvent::StepStart { .. } => "step_start",
            ExecutionEvent::StepFinish { .. } => "step_finish",
            ExecutionEvent::Tokens { .. } => "tokens",
            ExecutionEvent::SessionStart { .. } => "session_start",
            ExecutionEvent::SessionEnd { .. } => "session_end",
            ExecutionEvent::ModelSwitch { .. } => "model_switch",
            ExecutionEvent::Cost { .. } => "cost",
            ExecutionEvent::Duration { .. } => "duration",
            ExecutionEvent::Progress { .. } => "progress",
        }
    }

    /// 是否为需要前端特殊渲染的交互类型
    pub fn is_interactive(&self) -> bool {
        matches!(
            self,
            ExecutionEvent::ToolCall { .. }
                | ExecutionEvent::ToolResult { .. }
                | ExecutionEvent::Thinking { .. }
        )
    }

    /// 是否为需要显示在对话视图的消息类型
    pub fn is_message(&self) -> bool {
        matches!(
            self,
            ExecutionEvent::Assistant { .. }
                | ExecutionEvent::User { .. }
                | ExecutionEvent::System { .. }
        )
    }
}
```

#### 2.2.2 ExecutionMetadata 结构

```rust
// execution_events/metadata.rs

/// 执行元数据：跨事件的上下文信息
///
/// # 设计原则
/// - 累积模式：字段初始为 None，随着事件流逐步填充
/// - 线程安全：通过 Arc 共享，允许在事件处理中更新
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExecutionMetadata {
    // ── 标识信息 ──────────────────────────────────────
    /// 会话 ID
    pub session_id: Option<String>,
    /// 使用的模型
    pub model: Option<String>,
    /// 执行器类型
    pub executor: String,

    // ── Token 统计（累积） ─────────────────────────────
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,

    // ── 成本与耗时 ────────────────────────────────────
    pub cost_usd: f64,
    pub duration_ms: u64,

    // ── 时间戳 ─────────────────────────────────────────
    pub started_at: Option<String>,
    pub finished_at: Option<String>,

    // ── 执行状态 ──────────────────────────────────────
    pub exit_code: Option<i32>,
    pub is_success: bool,
}

impl ExecutionMetadata {
    pub fn new(executor: String) -> Self {
        Self {
            executor,
            ..Default::default()
        }
    }

    /// 从事件累积更新元数据
    pub fn update_from(&mut self, event: &ExecutionEvent) {
        match event {
            ExecutionEvent::Tokens { input, output, cache_read, cache_write } => {
                self.input_tokens = *input;
                self.output_tokens = *output;
                if let Some(cr) = cache_read {
                    self.cache_read_tokens = *cr;
                }
                if let Some(cw) = cache_write {
                    self.cache_write_tokens = *cw;
                }
            }
            ExecutionEvent::SessionStart { session_id } => {
                self.session_id = Some(session_id.clone());
            }
            ExecutionEvent::ModelSwitch { model } => {
                self.model = Some(model.clone());
            }
            ExecutionEvent::Cost { cost_usd } => {
                self.cost_usd = *cost_usd;
            }
            ExecutionEvent::Duration { duration_ms } => {
                self.duration_ms = *duration_ms;
            }
            ExecutionEvent::Progress { percent, message } => {
                // 进度可以触发通知等
                tracing::debug!("执行进度: {}% - {:?}", percent, message);
            }
            _ => {}
        }
    }

    /// 转换为数据库存储的 ExecutionUsage 格式
    pub fn to_usage(&self) -> crate::models::ExecutionUsage {
        crate::models::ExecutionUsage {
            input_tokens: self.input_tokens,
            output_tokens: self.output_tokens,
            cache_read_input_tokens: Some(self.cache_read_tokens),
            cache_creation_input_tokens: Some(self.cache_write_tokens),
            total_cost_usd: Some(self.cost_usd),
            duration_ms: Some(self.duration_ms),
        }
    }
}
```

#### 2.2.3 EventExtractor Trait

```rust
// execution_events/extractor.rs

/// 事件提取器 Trait：每个执行器对应一个实现
///
/// # 设计原则
/// - Send + Sync：允许跨线程使用
/// - 无状态或最小状态：提取逻辑应尽可能纯函数化
/// - 返回 Vec：某些行可能产生多个事件（如同时输出文本和 token）
#[async_trait]
pub trait EventExtractor: Send + Sync {
    /// 执行器类型名称
    fn executor_name(&self) -> &str;

    /// 从原始输出行提取事件列表
    ///
    /// # 参数
    /// - line: 原始输出行（不含换行符）
    ///
    /// # 返回
    /// - Vec<ExecutionEvent>：可能为空（行不产生事件）
    fn extract(&self, line: &str) -> Vec<ExecutionEvent>;

    /// 从原始错误输出行提取事件
    ///
    /// 默认实现：将错误行包装为 Error 事件
    fn extract_stderr(&self, line: &str) -> Option<ExecutionEvent> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return None;
        }
        Some(ExecutionEvent::Error {
            message: trimmed.to_string(),
        })
    }

    /// 获取当前累积的元数据（引用）
    fn metadata(&self) -> &ExecutionMetadata;

    /// 获取当前累积的元数据（可变引用）
    fn metadata_mut(&mut self) -> &mut ExecutionMetadata;
}
```

#### 2.2.4 EventPipeline

```rust
// execution_events/pipeline.rs

/// 事件处理管道
///
/// 负责：
/// - 接收原始输出行
/// - 调用 EventExtractor 转换为事件
/// - 累积元数据
/// - 生成下游所需的各类格式
pub struct EventPipeline {
    extractor: Box<dyn EventExtractor>,
    events: Vec<ExecutionEvent>,
}

impl EventPipeline {
    /// 创建新的管道
    pub fn new(executor: String) -> Self {
        Self {
            extractor: Self::create_extractor(&executor),
            events: Vec::new(),
        }
    }

    /// 根据执行器名称创建对应的提取器
    fn create_extractor(executor: &str) -> Box<dyn EventExtractor> {
        match executor {
            "claudecode" | "claude" => Box::new(claude_code::ClaudeCodeExtractor::new()),
            "kilo" => Box::new(kilo::KiloExtractor::new()),
            "opencode" => Box::new(opencode::OpencodeExtractor::new()),
            "mimo" => Box::new(mimo::MimoExtractor::new()),
            // ... 其他执行器
            _ => Box::new(default::DefaultExtractor::new(executor.to_string())),
        }
    }

    /// 处理一行标准输出
    pub fn feed(&mut self, line: &str) {
        let new_events = self.extractor.extract(line);
        for event in &new_events {
            self.extractor.metadata_mut().update_from(event);
        }
        self.events.extend(new_events);
    }

    /// 处理一行错误输出
    pub fn feed_stderr(&mut self, line: &str) {
        if let Some(event) = self.extractor.extract_stderr(line) {
            self.extractor.metadata_mut().update_from(&event);
            self.events.push(event);
        }
    }

    /// 结束处理，生成元数据事件
    pub fn finalize(&mut self) {
        let metadata = self.extractor.metadata();

        // 生成会话结束事件
        if let Some(session_id) = &metadata.session_id {
            self.events.push(ExecutionEvent::SessionEnd {
                session_id: session_id.clone(),
            });
        }

        // 生成最终的 tokens 事件（如果之前没有）
        if metadata.input_tokens > 0 || metadata.output_tokens > 0 {
            // 检查是否已有 tokens 事件
            let has_tokens = self.events.iter().any(|e| matches!(e, ExecutionEvent::Tokens { .. }));
            if !has_tokens {
                self.events.push(ExecutionEvent::Tokens {
                    input: metadata.input_tokens,
                    output: metadata.output_tokens,
                    cache_read: Some(metadata.cache_read_tokens),
                    cache_write: Some(metadata.cache_write_tokens),
                });
            }
        }
    }

    /// 获取所有已累积的事件
    pub fn events(&self) -> &[ExecutionEvent] {
        &self.events
    }

    /// 获取最后一条事件
    pub fn latest_event(&self) -> Option<&ExecutionEvent> {
        self.events.last()
    }

    /// 获取累积的元数据
    pub fn metadata(&self) -> &ExecutionMetadata {
        self.extractor.metadata()
    }

    /// 转换为数据库格式
    pub fn to_db_logs(&self) -> Vec<DbLogEntry> {
        self.events
            .iter()
            .map(|e| DbLogEntry::from_event(e))
            .collect()
    }

    /// 转换为 WebSocket 推送格式
    pub fn to_ws_events(&self) -> Vec<WsEvent> {
        self.events
            .iter()
            .map(|e| WsEvent::from_event(e))
            .collect()
    }

    /// 转换为飞书卡片格式
    pub fn to_feishu_card(&self) -> FeishuCard {
        FeishuCard::from_pipeline(self)
    }
}
```

#### 2.2.5 数据库适配

```rust
// execution_events/db_adapter.rs

/// 数据库日志条目
#[derive(Debug, Clone)]
pub struct DbLogEntry {
    pub timestamp: String,
    pub log_type: String,
    pub content: String,
    pub tool_name: Option<String>,
    pub tool_input_json: Option<String>,
    pub usage: Option<ExecutionUsage>,
}

impl DbLogEntry {
    pub fn from_event(event: &ExecutionEvent) -> Self {
        let timestamp = crate::models::utc_timestamp();

        match event {
            ExecutionEvent::ToolCall { id, name, input } => Self {
                timestamp,
                log_type: "tool_call".to_string(),
                content: name.clone(),
                tool_name: Some(name.clone()),
                tool_input_json: Some(input.to_string()),
                usage: None,
            },
            ExecutionEvent::ToolResult { call_id, output, is_error } => Self {
                timestamp,
                log_type: "tool_result".to_string(),
                content: output.clone(),
                tool_name: None,
                tool_input_json: None,
                usage: None,
            },
            ExecutionEvent::Thinking { content } => Self {
                timestamp,
                log_type: "thinking".to_string(),
                content: content.clone(),
                tool_name: None,
                tool_input_json: None,
                usage: None,
            },
            ExecutionEvent::Result { summary } => Self {
                timestamp,
                log_type: "result".to_string(),
                content: summary.clone(),
                tool_name: None,
                tool_input_json: None,
                usage: None,
            },
            // 其他事件类型映射...
            _ => Self {
                timestamp,
                log_type: event.to_log_type().to_string(),
                content: event.content_preview(),
                tool_name: None,
                tool_input_json: None,
                usage: None,
            },
        }
    }
}
```

### 2.3 各执行器的 Extractor 实现示例

#### 2.3.1 Claude Code Extractor

```rust
// execution_events/impls/claude_code.rs

/// Claude Code 执行器的事件提取器
///
/// Claude Code 使用 JSON lines 格式输出事件：
/// - {"type": "assistant", "message": {...}}
/// - {"type": "result", "usage": {...}}
pub struct ClaudeCodeExtractor {
    metadata: ExecutionMetadata,
    buffer: String,
}

impl ClaudeCodeExtractor {
    pub fn new() -> Self {
        Self {
            metadata: ExecutionMetadata::new("claude_code".to_string()),
            buffer: String::new(),
        }
    }
}

impl EventExtractor for ClaudeCodeExtractor {
    fn executor_name(&self) -> &str {
        "claude_code"
    }

    fn extract(&self, line: &str) -> Vec<ExecutionEvent> {
        // 尝试解析为 JSON
        if let Ok(msg) = serde_json::from_str::<claude_protocol::ClaudeMessage>(line) {
            return self.extract_from_message(&msg);
        }

        // 非 JSON 行作为 info
        if !line.trim().is_empty() {
            vec![ExecutionEvent::Info {
                message: line.to_string(),
            }]
        } else {
            vec![]
        }
    }

    fn extract_stderr(&self, line: &str) -> Option<ExecutionEvent> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return None;
        }
        // 检查是否包含 error 关键字
        if trimmed.to_lowercase().contains("error") {
            Some(ExecutionEvent::Error {
                message: trimmed.to_string(),
            })
        } else {
            Some(ExecutionEvent::Info {
                message: trimmed.to_string(),
            })
        }
    }

    fn metadata(&self) -> &ExecutionMetadata {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut ExecutionMetadata {
        &mut self.metadata
    }
}

impl ClaudeCodeExtractor {
    fn extract_from_message(&self, msg: &claude_protocol::ClaudeMessage) -> Vec<ExecutionEvent> {
        use claude_protocol::{ClaudeMessage, ClaudeContentBlock};

        match msg {
            ClaudeMessage::Assistant { message, session_id, .. } => {
                let mut events = Vec::new();

                // 提取 thinking
                for block in &message.content {
                    if let ClaudeContentBlock::Thinking { thinking } = block {
                        if let Some(text) = thinking {
                            events.push(ExecutionEvent::Thinking {
                                content: text.clone(),
                            });
                        }
                    }
                }

                // 提取 tool_use
                for block in &message.content {
                    if let ClaudeContentBlock::ToolUse { id, name, input } = block {
                        events.push(ExecutionEvent::ToolCall {
                            id: id.clone().unwrap_or_default(),
                            name: name.clone().unwrap_or_default(),
                            input: input.clone(),
                        });
                    }
                }

                // 提取普通文本
                let texts: Vec<String> = message
                    .content
                    .iter()
                    .filter_map(|b| {
                        if let ClaudeContentBlock::Text { text } = b {
                            text.clone()
                        } else {
                            None
                        }
                    })
                    .collect();

                if !texts.is_empty() {
                    events.push(ExecutionEvent::Assistant {
                        content: texts.join("\n"),
                        thinking: None,
                        message_id: message.id.clone(),
                    });
                }

                if let Some(sid) = session_id {
                    self.metadata_mut().session_id = Some(sid.clone());
                }

                events
            }
            ClaudeMessage::Result { usage, total_cost_usd, duration_ms, result, .. } => {
                let mut events = Vec::new();

                if let Some(usage) = usage {
                    self.metadata_mut().input_tokens = usage.input_tokens;
                    self.metadata_mut().output_tokens = usage.output_tokens;
                    if let Some(cr) = usage.cache_read_input_tokens {
                        self.metadata_mut().cache_read_tokens = cr;
                    }
                    if let Some(cw) = usage.cache_creation_input_tokens {
                        self.metadata_mut().cache_write_tokens = cw;
                    }
                    events.push(ExecutionEvent::Tokens {
                        input: usage.input_tokens,
                        output: usage.output_tokens,
                        cache_read: usage.cache_read_input_tokens,
                        cache_write: usage.cache_creation_input_tokens,
                    });
                }

                if let Some(cost) = total_cost_usd {
                    self.metadata_mut().cost_usd = *cost;
                    events.push(ExecutionEvent::Cost { cost_usd: *cost });
                }

                if let Some(dur) = duration_ms {
                    self.metadata_mut().duration_ms = *dur;
                    events.push(ExecutionEvent::Duration { duration_ms: *dur });
                }

                if let Some(summary) = result {
                    events.push(ExecutionEvent::Result {
                        summary: summary.clone(),
                    });
                }

                events
            }
            _ => vec![],
        }
    }
}
```

#### 2.3.2 Kilo Extractor

```rust
// execution_events/impls/kilo.rs

/// Kilo 执行器的事件提取器
///
/// Kilo 使用 SSE/JSON 格式：
/// - {"type": "agent", "part": {"type": "text", "text": "..."}}
/// - {"type": "agent", "part": {"type": "tool_call", ...}}
/// - {"type": "agent", "part": {"type": "tool_result", ...}}
/// - {"type": "result", "tokens": {...}}
pub struct KiloExtractor {
    metadata: ExecutionMetadata,
}

impl KiloExtractor {
    pub fn new() -> Self {
        Self {
            metadata: ExecutionMetadata::new("kilo".to_string()),
        }
    }
}

impl EventExtractor for KiloExtractor {
    fn executor_name(&self) -> &str {
        "kilo"
    }

    fn extract(&self, line: &str) -> Vec<ExecutionEvent> {
        if let Ok(event) = serde_json::from_str::<kilo_event::KiloAgentEvent>(line) {
            self.extract_from_event(&event)
        } else if !line.trim().is_empty() {
            vec![ExecutionEvent::Info {
                message: line.to_string(),
            }]
        } else {
            vec![]
        }
    }

    fn metadata(&self) -> &ExecutionMetadata {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut ExecutionMetadata {
        &mut self.metadata
    }
}

impl KiloExtractor {
    fn extract_from_event(&self, event: &kilo_event::KiloAgentEvent) -> Vec<ExecutionEvent> {
        use kilo_event::KiloAgentEvent;

        match event.event_type.as_str() {
            "agent" => {
                if let Some(part) = &event.part {
                    match part.part_type.as_deref() {
                        Some("text") => {
                            if let Some(text) = &part.text {
                                return vec![ExecutionEvent::Assistant {
                                    content: text.clone(),
                                    thinking: None,
                                    message_id: part.message_id.clone(),
                                }];
                            }
                        }
                        Some("tool_call") | Some("tool_use") => {
                            if let Some(state) = &part.state {
                                let name = state.input.as_ref()
                                    .and_then(|i| i.command.clone())
                                    .unwrap_or_else(|| part.tool.clone().unwrap_or_default());
                                return vec![ExecutionEvent::ToolCall {
                                    id: part.call_id.clone().unwrap_or_default(),
                                    name,
                                    input: serde_json::to_value(&state.input).unwrap_or_default(),
                                }];
                            }
                        }
                        Some("tool_result") => {
                            if let Some(state) = &part.state {
                                return vec![ExecutionEvent::ToolResult {
                                    call_id: part.call_id.clone().unwrap_or_default(),
                                    output: state.output.clone().unwrap_or_default(),
                                    is_error: false,
                                }];
                            }
                        }
                        Some("reasoning") | Some("thinking") => {
                            if let Some(reason) = &part.reason {
                                return vec![ExecutionEvent::Thinking {
                                    content: reason.clone(),
                                }];
                            }
                        }
                        _ => {}
                    }
                }
                vec![]
            }
            "result" => {
                if let Some(part) = &event.part {
                    if let Some(tokens) = &part.tokens {
                        self.metadata_mut().input_tokens = tokens.input;
                        self.metadata_mut().output_tokens = tokens.output;
                        self.metadata_mut().cache_read_tokens = tokens.cache.read;
                        self.metadata_mut().cache_write_tokens = tokens.cache.write;

                        if let Some(cost) = part.cost {
                            self.metadata_mut().cost_usd = cost;
                        }

                        return vec![ExecutionEvent::Tokens {
                            input: tokens.input,
                            output: tokens.output,
                            cache_read: Some(tokens.cache.read),
                            cache_write: Some(tokens.cache.write),
                        }];
                    }
                }
                vec![]
            }
            _ => vec![],
        }
    }
}
```

### 2.4 与现有系统的集成

#### 2.4.1 executor_service 改造

```rust
// backend/src/executor_service/mod.rs

// 移除对 ParsedLogEntry 的直接依赖
// 改用 ExecutionEvent

pub async fn run_spawned_executor_task(/* ... */) {
    let mut pipeline = EventPipeline::new(executor.executor_type().to_string());

    // stdout 循环
    loop {
        tokio::select! {
            line = stdout.recv() => {
                match line {
                    Ok(l) => {
                        pipeline.feed(&l);
                        // 实时推送最新事件
                        if let Some(event) = pipeline.latest_event() {
                            tx.send(ExecEvent::Output {
                                task_id: task_id.clone(),
                                event: event.clone(),
                            }).unwrap();
                        }
                    }
                    Err(_) => break,
                }
            }
            err_line = stderr.recv() => {
                if let Ok(l) = err_line {
                    pipeline.feed_stderr(&l);
                }
            }
            _ = cancel_rx.recv() => {
                // 处理取消
                break;
            }
        }
    }

    // 结束处理
    pipeline.finalize();

    // 更新数据库
    let db_entries = pipeline.to_db_logs();
    let metadata = pipeline.metadata();

    db.update_execution_record(UpdateExecutionRecordRequest {
        id: record_id,
        status: if metadata.is_success { "success" } else { "failed" },
        remaining_logs: &serde_json::to_string(&db_entries).unwrap_or_default(),
        result: pipeline.events()
            .iter()
            .rev()
            .find(|e| matches!(e, ExecutionEvent::Result { .. }))
            .map(|e| e.content_preview())
            .unwrap_or_default(),
        usage: Some(&metadata.to_usage()),
        model: metadata.model.as_deref(),
        review_meta: None,
    }).await?;

    // 飞书推送
    if let Some(feishu_card) = feishu_config.map(|_| pipeline.to_feishu_card()) {
        feishu_push_service.push(feishu_card).await;
    }
}
```

#### 2.4.2 ExecEvent 改造

```rust
// backend/src/handlers/mod.rs

// ExecEvent 增加事件类型的变体
#[derive(Debug, Clone)]
pub enum ExecEvent {
    Started { task_id: String, todo_id: i64, todo_title: String, executor: String },
    // 改为结构化事件
    Output { task_id: String, event: ExecutionEvent },
    Finished {
        task_id: String,
        todo_id: i64,
        todo_title: String,
        executor: String,
        success: bool,
        result: Option<String>,
        metadata: ExecutionMetadata,
        events: Vec<ExecutionEvent>,
        feishu_bot_id: Option<i64>,
        feishu_receive_id: Option<String>,
        workspace_id: Option<i64>,
    },
    // ... 其他变体
}
```

---

## 3. 数据库设计（保持不变）

### 3.1 execution_logs 表

```sql
-- 保持现有结构不变
CREATE TABLE execution_logs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    record_id INTEGER NOT NULL,
    timestamp TEXT NOT NULL,
    log_type TEXT NOT NULL,  -- 使用 ExecutionEvent.to_log_type() 的返回值
    content TEXT NOT NULL,
    metadata TEXT,  -- 仍使用 JSON 存储 tool_name, tool_input_json, usage
    FOREIGN KEY (record_id) REFERENCES execution_records(id)
);

-- 索引
CREATE INDEX idx_execution_logs_record_id ON execution_logs(record_id);
CREATE INDEX idx_execution_logs_log_type ON execution_logs(log_type);
```

### 3.2 log_type 统一值

| ExecutionEvent | log_type | 说明 |
|----------------|----------|------|
| Assistant | assistant | 助手消息 |
| Thinking | thinking | 思考过程 |
| ToolCall | tool_call | 工具调用 |
| ToolResult | tool_result | 工具结果 |
| Result | result | 最终结果 |
| User | user | 用户消息 |
| System | system | 系统消息 |
| Info | info | 普通信息 |
| Error | error | 错误信息 |
| StepStart | step_start | 步骤开始 |
| StepFinish | step_finish | 步骤完成 |
| Tokens | tokens | Token 统计 |
| SessionStart | session_start | 会话开始 |
| SessionEnd | session_end | 会话结束 |
| ModelSwitch | model_switch | 模型切换 |
| Cost | cost | 成本 |
| Duration | duration | 耗时 |
| Progress | progress | 进度 |

---

## 4. 前端适配

### 4.1 类型定义

```typescript
// frontend/src/types/execution.tsx

// 新的统一事件类型
export type ExecutionEventType =
  | 'assistant'
  | 'thinking'
  | 'tool_call'
  | 'tool_result'
  | 'result'
  | 'user'
  | 'system'
  | 'info'
  | 'error'
  | 'step_start'
  | 'step_finish'
  | 'tokens'
  | 'session_start'
  | 'session_end'
  | 'model_switch'
  | 'cost'
  | 'duration'
  | 'progress';

export interface ExecutionEvent {
  type: ExecutionEventType;
  data: ExecutionEventData;
}

export type ExecutionEventData =
  | { content: string; thinking?: string; message_id?: string }  // Assistant
  | { content: string }                                          // Thinking
  | { id: string; name: string; input: Record<string, unknown> } // ToolCall
  | { call_id: string; output: string; is_error: boolean }       // ToolResult
  | { summary: string }                                          // Result
  | { message: string }                                          // Info/Error/System
  | { input: number; output: number; cache_read?: number; cache_write?: number } // Tokens
  | { session_id: string }                                       // SessionStart/End
  | { model: string }                                           // ModelSwitch
  | { cost_usd: number }                                        // Cost
  | { duration_ms: number }                                     // Duration
  | { percent: number; message?: string }                        // Progress
  | { name: string; index: number }                              // StepStart/Finish
  | { content: string }                                         // User
  ;

export interface ExecutionMetadata {
  session_id?: string;
  model?: string;
  executor: string;
  input_tokens: number;
  output_tokens: number;
  cache_read_tokens: number;
  cache_write_tokens: number;
  cost_usd: number;
  duration_ms: number;
  started_at?: string;
  finished_at?: string;
  exit_code?: number;
  is_success: boolean;
}
```

### 4.2 LogEntry 兼容层

```typescript
// 从 ExecutionEvent 转换为 LogEntry
export function toLogEntry(event: ExecutionEvent): LogEntry {
  return {
    timestamp: new Date().toISOString(),
    type: event.type,
    content: extractContent(event),
    toolName: extractToolName(event),
    toolInputJson: extractToolInput(event),
    toolResult: extractToolResult(event),
    isError: event.type === 'error',
  };
}
```

---

## 5. 实施计划

### Phase 1: 模块骨架（1-2 天）

- [ ] 创建 `execution_events` 模块目录结构
- [ ] 定义 `ExecutionEvent` 枚举
- [ ] 定义 `ExecutionMetadata` 结构
- [ ] 定义 `EventExtractor` trait
- [ ] 实现 `EventPipeline`
- [ ] 实现 `DbLogEntry` 和数据库适配
- [ ] 实现基础的 `DefaultExtractor`

### Phase 2: 核心执行器（3-5 天）

- [ ] 实现 `ClaudeCodeExtractor`
- [ ] 实现 `KiloExtractor`
- [ ] 实现 `OpencodeExtractor`
- [ ] 验证与现有解析逻辑的兼容性

### Phase 3: 管道集成（2-3 天）

- [ ] 修改 `executor_service` 使用 `EventPipeline`
- [ ] 更新 `ExecEvent` 枚举
- [ ] 更新 WebSocket 推送逻辑
- [ ] 更新飞书推送适配器

### Phase 4: 前端适配（2-3 天）

- [ ] 更新前端类型定义
- [ ] 更新日志渲染组件
- [ ] 验证三种视图（log/chat/command）正常显示

### Phase 5: 完善其他执行器（3-5 天）

- [ ] 实现剩余执行器的 Extractor
- [ ] 统一 log_type 命名
- [ ] 清理旧的解析代码

---

## 6. 风险与缓解

| 风险 | 影响 | 缓解措施 |
|------|------|----------|
| 破坏现有解析逻辑 | 高 | Phase 2 先为每个执行器编写独立的 Extractor，保持双轨运行 |
| 前端需要大量适配 | 中 | 提供 LogEntry 兼容层，平滑迁移 |
| 性能下降 | 低 | EventPipeline 使用 Vec::new()，避免不必要的分配 |
| 数据库迁移 | 低 | 保持表结构不变，只改变 log_type 值 |

---

## 7. 附录

### 7.1 各执行器协议参考

| 执行器 | 协议格式 | 关键事件 |
|--------|----------|----------|
| claude_code | JSON lines | assistant, tool_use, tool_result, result |
| kilo | JSON + SSE | agent (text, tool_call, tool_result), result |
| opencode | JSON + SSE | step-start, tool-use, tool-result |
| mimo | 特殊格式 | 参考 mimo_event.rs |
| 其他 | 各有不同 | 参考各 adapter_event.rs |

### 7.2 参考文件

- `backend/src/models/mod.rs` - 当前 ParsedLogEntry, ExecutionUsage 定义
- `backend/src/adapters/mod.rs` - 各执行器协议解析
- `backend/src/adapters/agent_event.rs` - Agent 事件定义
- `backend/src/adapters/claude_protocol.rs` - Claude Code 协议定义
- `backend/src/services/feishu_push.rs` - 飞书推送
- `backend/src/handlers/mod.rs` - ExecEvent 定义
