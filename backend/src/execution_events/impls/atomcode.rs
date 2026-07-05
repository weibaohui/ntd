//! AtomCode 执行器的事件提取器实现
//!
//! AtomCode 的输出比较特殊——不是 JSON 格式。
//! stdout 是纯文本（AI 回复），stderr 包含以 `[xxx]` 前缀标记的结构化事件。
//!
//! 文本输出特点：多行无 `[xx]` 前缀的纯文本行穿插在结构化事件中，
//! 且文本末尾可能紧贴下一行结构化事件（无换行），如：
//!   当前任务已完成[tokens] prompt=100 completion=50
//!
//! 事件类型：
//! - `[tokens] prompt=N completion=M` → Tokens 事件
//! - `[done] <duration> tokens=N turns=N tool_calls=N` → StepFinish + 元数据更新
//! - `[tool→ <name> args={"key":"val"}]` → ToolCall 事件
//! - `[tool← <name> <status> <duration>] <result>` → ToolResult 事件
//! - `[thinking] <text>` → 思考内容（多行累积为一块直到非 [thinking] 行）
//! - `[engine v2] new stack active (model xxx)` → ModelSwitch 事件
//! - 无前缀纯文本行 → Assistant 事件（多行累积为一块）

use crate::execution_events::event::ExecutionEvent;
use crate::execution_events::extractor::EventExtractor;
use crate::execution_events::metadata::ExecutionMetadata;

/// 已知的结构化事件前缀（不含尾部 `]`，便于行内匹��）
const STRUCTURED_MARKERS: &[&str] = &[
    "[tokens",
    "[done",
    "[tool→",
    "[tool->",
    "[tool←",
    "[tool<-",
    "[thinking",
    "[tool-streaming",
    "[headless",
    "[approval-denied",
    "[engine",
];

/// 在文本行中查找第一个已知结构化标记的位置
///
/// 返回 `(text_part, structured_suffix)`，其中 text_part 是标记之前的内容，
/// structured_suffix 是从标记开始到行尾。如果没有标记则返回 None。
fn find_structured_split(line: &str) -> Option<(&str, &str)> {
    for marker in STRUCTURED_MARKERS {
        if let Some(pos) = line.find(marker) {
            // 确保是真正的结构��标记（前面不是字母数字，避免误匹配）
            if pos == 0 || !line.as_bytes()[pos - 1].is_ascii_alphanumeric() {
                let text = line[..pos].trim();
                let structured = &line[pos..];
                return Some((text, structured));
            }
        }
    }
    None
}

/// AtomCode 事件提取器
#[derive(Debug, Clone)]
pub struct AtomcodeExtractor {
    metadata: ExecutionMetadata,
    /// 工具调用序号（递增，保证每次工具调用有唯一 ID）
    tool_seq: u64,
    /// 思考块缓冲：多行 [thinking] 累积为一块，非 [thinking] 行触发 flush
    pending_thinking: Vec<String>,
    /// 纯文本缓冲：多行无前缀文本累积为一块 Assistant 事件
    pending_text: Vec<String>,
}

impl AtomcodeExtractor {
    pub fn new() -> Self {
        Self {
            metadata: ExecutionMetadata::new("atomcode".to_string()),
            tool_seq: 0,
            pending_thinking: Vec::new(),
            pending_text: Vec::new(),
        }
    }

    /// 解析结构化事件行（以 `[xxx]` 开头）
    fn parse_stderr_line(&mut self, trimmed: &str) -> Vec<ExecutionEvent> {
        let mut events = Vec::new();

        // 思考块处理：多行 [thinking]/[THINK] 累积为一块
        // 使用 strip_prefix 替代手动索引切片，避免越界并让意图更清晰
        if let Some(content) = trimmed.strip_prefix("[thinking]") {
            let content = content.trim();
            self.pending_thinking.push(content.to_string());
            return events;
        }
        if let Some(content) = trimmed.strip_prefix("[THINK]") {
            let content = content.trim();
            self.pending_thinking.push(content.to_string());
            return events;
        }

        // 非 thinking 行，先 flush 之前缓冲的思考块和文本块
        self.flush_thinking(&mut events);
        self.flush_text(&mut events);

        // 跳过流式/headless 标记
        if trimmed.starts_with("[tool-streaming") || trimmed.starts_with("[headless]") || trimmed.starts_with("[tool-batch") {
            return events;
        }

        // 引擎信息行: [engine v2] new stack active (model deepseek-v4-flash)
        if trimmed.starts_with("[engine") {
            if let Some(pos) = trimmed.find("(model ") {
                let after = &trimmed[pos + 7..];
                let model = after.trim_end_matches(')').trim();
                if !model.is_empty()
                    && self.metadata.model.is_none()
                {
                    self.metadata.model = Some(model.to_string());
                    events.push(ExecutionEvent::ModelSwitch {
                        model: model.to_string(),
                    });
                }
            }
            return events;
        }

        if trimmed.starts_with("[tokens]") {
            let mut prompt_tokens = 0u64;
            let mut completion_tokens = 0u64;
            for part in trimmed.split_whitespace().skip(1) {
                if let Some((key, val)) = part.split_once('=') {
                    match key {
                        "prompt" => prompt_tokens = val.parse().unwrap_or(0),
                        "completion" => completion_tokens = val.parse().unwrap_or(0),
                        _ => {}
                    }
                }
            }
            events.push(ExecutionEvent::Tokens {
                input: prompt_tokens,
                output: completion_tokens,
                cache_read: None,
                cache_write: None,
            });
        } else if trimmed.starts_with("[done]") {
            let mut duration_ms = None;
            let mut total_tokens = 0u64;
            for (i, part) in trimmed.split_whitespace().enumerate() {
                if i == 1 {
                    let s = part.trim_end_matches('s');
                    if let Ok(secs) = s.parse::<f64>() {
                        duration_ms = Some((secs * 1000.0) as u64);
                    }
                } else if let Some((key, val)) = part.split_once('=') {
                    if key == "tokens" {
                        total_tokens = val.parse().unwrap_or(0);
                    }
                }
            }

            if let Some(ms) = duration_ms {
                self.metadata.duration_ms = ms;
            }
            if total_tokens > 0 {
                self.metadata.input_tokens = total_tokens;
            }
            self.metadata.set_finished_at();

            events.push(ExecutionEvent::StepFinish {
                name: "execution".to_string(),
                index: 0,
            });
        } else if trimmed.starts_with("[tool→") || trimmed.starts_with("[tool->")
        {
            let content = trimmed
                .trim_start_matches("[tool→")
                .trim_start_matches("[tool->")
                .trim();

            let (name, args_json) = if let Some(idx) = content.find(" args=") {
                let name = content[..idx].trim().to_string();
                let raw_args = content[idx + 6..].trim().trim_end_matches(']');
                let parsed = if raw_args.starts_with('{') {
                    serde_json::from_str(raw_args).ok()
                } else {
                    None
                };
                (name, parsed.unwrap_or(serde_json::json!({})))
            } else {
                (content.trim_end_matches(']').to_string(), serde_json::json!({}))
            };

            self.tool_seq += 1;
            let tool_id = format!("tool_{}", self.tool_seq);

            events.push(ExecutionEvent::ToolCall {
                id: tool_id,
                name,
                input: args_json,
            });
        } else if trimmed.starts_with("[tool←") || trimmed.starts_with("[tool<-")
        {
            let content = trimmed
                .trim_start_matches("[tool←")
                .trim_start_matches("[tool<-")
                .trim();

            let (meta_part, result_text) = if let Some(idx) = content.find(']') {
                (content[..idx].trim(), content[idx + 1..].trim().to_string())
            } else {
                (content, String::new())
            };

            let parts: Vec<&str> = meta_part.split_whitespace().collect();
            let _name = if !parts.is_empty() { parts[0] } else { "" };
            let status = parts.get(1).copied().unwrap_or("OK");
            let is_error = status != "OK";

            events.push(ExecutionEvent::ToolResult {
                call_id: format!("tool_{}", self.tool_seq),
                output: result_text,
                is_error,
            });
        } else if trimmed.starts_with("[approval-denied]") {
            events.push(ExecutionEvent::Error {
                message: trimmed.to_string(),
            });
        }

        events
    }

    /// 将缓冲的思考行合并为一个 Thinking 事件
    fn flush_thinking(&mut self, events: &mut Vec<ExecutionEvent>) {
        if self.pending_thinking.is_empty() {
            return;
        }
        let content = self.pending_thinking.join("\n");
        self.pending_thinking.clear();
        if !content.trim().is_empty() {
            events.push(ExecutionEvent::Thinking { content });
        }
    }

    /// 将缓冲的纯文本行合并为一个 Result 事件（执行结论）
    fn flush_text(&mut self, events: &mut Vec<ExecutionEvent>) {
        if self.pending_text.is_empty() {
            return;
        }
        let content = self.pending_text.join("\n");
        self.pending_text.clear();
        if !content.trim().is_empty() {
            events.push(ExecutionEvent::Result {
                summary: content,
            });
        }
    }
}

impl EventExtractor for AtomcodeExtractor {
    fn executor_name(&self) -> &str {
        "atomcode"
    }

    fn extract(&mut self, line: &str) -> Vec<ExecutionEvent> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return Vec::new();
        }

        let mut events = Vec::new();

        // 以 `[` 开头：结构化事件行
        if trimmed.starts_with('[') {
            self.flush_text(&mut events);
            events.extend(self.parse_stderr_line(trimmed));
            return events;
        }

        // 不以 `[` 开头，先 flush 思考块（单行或多行累积的 thinking）
        self.flush_thinking(&mut events);

        // 行内可能混入结构化标记（文本末尾紧贴下一行）
        // 如：当前任务已完成[tokens] prompt=100 completion=50
        if let Some((text_part, structured_part)) = find_structured_split(trimmed) {
            // 累积文本部分
            if !text_part.is_empty() {
                self.pending_text.push(text_part.to_string());
            }
            // flush 文本块 + 解析结构化部分
            self.flush_text(&mut events);
            events.extend(self.parse_stderr_line(structured_part));
            return events;
        }

        // 普通纯文本行：累积等待 flush
        self.pending_text.push(trimmed.to_string());
        events
    }

    fn extract_stderr(&mut self, line: &str) -> Option<ExecutionEvent> {
        // atomcode 的 stderr 由 try_parse_stderr_with_pipeline 通过 pipeline.feed()
        // 驱动 extract()，本方法仅在 fallback 路径被调用。为保持一致，委托给 extract()
        self.extract(line).into_iter().next()
    }

    fn metadata(&self) -> &ExecutionMetadata {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut ExecutionMetadata {
        &mut self.metadata
    }
}

impl Default for AtomcodeExtractor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokens_line() {
        let mut ext = AtomcodeExtractor::new();
        let events = ext.extract("[tokens] prompt=120 completion=45");
        assert_eq!(events.len(), 1);
        match &events[0] {
            ExecutionEvent::Tokens { input, output, .. } => {
                assert_eq!(*input, 120);
                assert_eq!(*output, 45);
            }
            _ => panic!("Expected Tokens event"),
        }
    }

    #[test]
    fn test_done_line() {
        let mut ext = AtomcodeExtractor::new();
        let events = ext.extract("[done] 4.6s tokens=100 turns=2 tool_calls=1");
        assert_eq!(events.len(), 1);
        assert!(matches!(&events[0], ExecutionEvent::StepFinish { .. }));
        assert_eq!(ext.metadata().duration_ms, 4600);
    }

    #[test]
    fn test_tool_call() {
        let mut ext = AtomcodeExtractor::new();
        let events = ext.extract(r#"[tool→ bash args={"command": "ls -la"}]"#);
        assert_eq!(events.len(), 1);
        match &events[0] {
            ExecutionEvent::ToolCall { id, name, input, .. } => {
                assert_eq!(name, "bash");
                assert_eq!(id, "tool_1");
                assert_eq!(input.get("command").and_then(|v| v.as_str()), Some("ls -la"));
            }
            _ => panic!("Expected ToolCall event"),
        }
    }

    #[test]
    fn test_tool_result_ok() {
        let mut ext = AtomcodeExtractor::new();
        let _ = ext.extract(r#"[tool→ bash args={"command": "date"}]"#);
        let events = ext.extract("[tool← bash OK 10ms] Tue Jun 30 14:45:43 CST 2026");
        assert_eq!(events.len(), 1);
        match &events[0] {
            ExecutionEvent::ToolResult { call_id, output, is_error } => {
                assert_eq!(call_id, "tool_1");
                assert_eq!(output, "Tue Jun 30 14:45:43 CST 2026");
                assert!(!is_error);
            }
            _ => panic!("Expected ToolResult event"),
        }
    }

    #[test]
    fn test_tool_result_error() {
        let mut ext = AtomcodeExtractor::new();
        let _ = ext.extract(r#"[tool→ bash args={"command": "nonexistent"}]"#);
        let events = ext.extract("[tool← bash ERR 5ms] command not found");
        assert_eq!(events.len(), 1);
        match &events[0] {
            ExecutionEvent::ToolResult { output, is_error, .. } => {
                assert_eq!(output, "command not found");
                assert!(is_error);
            }
            _ => panic!("Expected ToolResult event"),
        }
    }

    #[test]
    fn test_tool_id_increments() {
        let mut ext = AtomcodeExtractor::new();
        let events1 = ext.extract(r#"[tool→ bash args={"command":"1"}]"#);
        let events2 = ext.extract(r#"[tool→ todo args={"action":"add"}]"#);
        match &events1[0] {
            ExecutionEvent::ToolCall { id, .. } => assert_eq!(id, "tool_1"),
            _ => panic!(),
        }
        match &events2[0] {
            ExecutionEvent::ToolCall { id, .. } => assert_eq!(id, "tool_2"),
            _ => panic!(),
        }
    }

    #[test]
    fn test_empty_line() {
        let mut ext = AtomcodeExtractor::new();
        assert!(ext.extract("").is_empty());
        assert!(ext.extract("   ").is_empty());
    }

    #[test]
    fn test_streaming_marker_skipped() {
        let mut ext = AtomcodeExtractor::new();
        assert!(ext.extract("[tool-streaming...]").is_empty());
        assert!(ext.extract("[headless]").is_empty());
    }

    #[test]
    fn test_engine_model_extraction() {
        let mut ext = AtomcodeExtractor::new();
        let events = ext.extract("[engine v2] new stack active (model deepseek-v4-flash)");
        assert_eq!(events.len(), 1);
        assert!(matches!(&events[0], ExecutionEvent::ModelSwitch { model } if model == "deepseek-v4-flash"));
        assert_eq!(ext.metadata().model.as_deref(), Some("deepseek-v4-flash"));
    }

    #[test]
    fn test_engine_model_only_once() {
        let mut ext = AtomcodeExtractor::new();
        let events1 = ext.extract("[engine v2] new stack active (model deepseek-v4-flash)");
        assert_eq!(events1.len(), 1);
        let events2 = ext.extract("[engine v2] new stack active (model other-model)");
        assert_eq!(events2.len(), 0);
        assert_eq!(ext.metadata().model.as_deref(), Some("deepseek-v4-flash"));
    }

    #[test]
    fn test_thinking_accumulation() {
        let mut ext = AtomcodeExtractor::new();
        assert!(ext.extract("[thinking] line 1").is_empty());
        assert!(ext.extract("[thinking] line 2").is_empty());
        assert!(ext.extract("[thinking] line 3").is_empty());

        let events = ext.extract("[done] 1s turns=1 tool_calls=0");
        assert!(events.iter().any(|e| matches!(e, ExecutionEvent::Thinking { content } if content == "line 1\nline 2\nline 3")));
        assert!(events.iter().any(|e| matches!(e, ExecutionEvent::StepFinish { .. })));
    }

    #[test]
    fn test_thinking_reset_between_blocks() {
        let mut ext = AtomcodeExtractor::new();
        assert!(ext.extract("[thinking] block 1").is_empty());
        let events1 = ext.extract("[done] 1s turns=1 tool_calls=0");
        assert!(events1.iter().any(|e| matches!(e, ExecutionEvent::Thinking { content } if content == "block 1")));

        assert!(ext.extract("[thinking] block 2").is_empty());
        let events2 = ext.extract("[done] 1s turns=1 tool_calls=0");
        assert!(events2.iter().any(|e| matches!(e, ExecutionEvent::Thinking { content } if content == "block 2")));
    }

    // ── 文本块积累测试 ──────────────────────────────────────

    #[test]
    fn test_text_accumulation_to_assistant() {
        // 多行无前缀文本累积为单个 Assistant 事件
        let mut ext = AtomcodeExtractor::new();
        assert!(ext.extract("第一段文本").is_empty());
        assert!(ext.extract("第二段文本").is_empty());
        assert!(ext.extract("第三段文本").is_empty());

        // 结构化行触发 flush
        let events = ext.extract("[tokens] prompt=10 completion=5");
        assert!(events.iter().any(|e| matches!(e, ExecutionEvent::Result { summary } if summary == "第一段文本\n第二段文本\n第三段文本")));
        assert!(events.iter().any(|e| matches!(e, ExecutionEvent::Tokens { .. })));
    }

    #[test]
    fn test_text_accumulation_flushed_by_structured() {
        // [xx] 行到达时 flush 之前的文本块
        let mut ext = AtomcodeExtractor::new();
        let events1 = ext.extract("AI 输出文本");
        assert!(events1.is_empty());

        let events2 = ext.extract("[done] 1s turns=1 tool_calls=0");
        assert_eq!(events2.len(), 2); // Assistant + StepFinish
        assert!(matches!(&events2[0], ExecutionEvent::Result { summary } if summary == "AI 输出文本"));
    }

    #[test]
    fn test_embedded_structured_split() {
        // 文本末尾紧贴结构化标记: "当前任务已完成[tokens] prompt=10 completion=5"
        let mut ext = AtomcodeExtractor::new();
        let events = ext.extract("当前任务已完成[tokens] prompt=10 completion=5");
        assert_eq!(events.len(), 2);
        assert!(matches!(&events[0], ExecutionEvent::Result { summary } if summary == "当前任务已完成"));
        assert!(matches!(&events[1], ExecutionEvent::Tokens { .. }));
    }

    #[test]
    fn test_embedded_structured_with_accumulated_text() {
        // 多条文本行 + 末尾拼贴结构化
        let mut ext = AtomcodeExtractor::new();
        assert!(ext.extract("第一行").is_empty());
        assert!(ext.extract("第二行").is_empty());

        let events = ext.extract("第三行结尾[tokens] prompt=5 completion=3");
        assert!(events.iter().any(|e| matches!(e, ExecutionEvent::Result { summary } if summary == "第一行\n第二行\n第三行结尾")));
        assert!(events.iter().any(|e| matches!(e, ExecutionEvent::Tokens { .. })));
    }

    #[test]
    fn test_thinking_flushed_by_plain_text() {
        // 单行 thinking 后紧跟纯文本，thinking 应被 flush
        let mut ext = AtomcodeExtractor::new();
        assert!(ext.extract("[thinking] All done. Let me summarize.").is_empty());

        let events = ext.extract("Some plain text output");
        // thinking 已 flush，但文本累积在 pending_text 中，不立即输出
        assert!(events.iter().any(|e| matches!(e, ExecutionEvent::Thinking { content } if content == "All done. Let me summarize.")));
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn test_thinking_flushed_by_embedded_marker() {
        // thinking 后紧跟嵌入式标记行，thinking 应被 flush
        let mut ext = AtomcodeExtractor::new();
        assert!(ext.extract("[thinking] Done.").is_empty());

        let events = ext.extract("任务完成[tokens] prompt=1 completion=1");
        assert!(events.iter().any(|e| matches!(e, ExecutionEvent::Thinking { content } if content == "Done.")));
        assert!(events.iter().any(|e| matches!(e, ExecutionEvent::Result { summary: _ })));
        assert!(events.iter().any(|e| matches!(e, ExecutionEvent::Tokens { .. })));
    }

    /// 完整流程测试: thinking + 多行文本 + 嵌入式 [tokens] → 聚合 Assistant + 结构化事件
    #[test]
    fn test_full_text_aggregation_with_thinking_and_tokens() {
        let mut ext = AtomcodeExtractor::new();

        // thinking 行 — 累积，不输出
        assert!(ext.extract("[thinking] All done. Let me summarize what happened.").is_empty());

        // 多行纯文本 — 累积到 pending_text，不输出（第一行除外，它会 flush thinking）
        let text_lines = [
            "全部完成，执行过程总结：",
            "| 步骤 | 命令/操作 | 结果 |",
            "|------|-----------|------|",
            "| 1️⃣ | `date` | `Tue Jun 30 16:21:03 CST 2026` |",
            "| 2️⃣ | `whoami` | `weibh` |",
            "| 3️⃣ | `todo add` 创建任务 | 任务 #1 创建，状态 `pending` → `[ ]` |",
            "| 4️⃣ | `todo update → in_progress` | 状态切换为 `[>]` |",
            "| 5️⃣ | `sleep 1` | 等待 1 秒 |",
            "| 6️⃣ | `todo update → completed` | 状态切换为 `[x]` ✅ |",
        ];
        // 第一个文本行触发 flush_thinking，输出 Thinking 事件
        let events_first = ext.extract(text_lines[0]);
        assert!(events_first.iter().any(|e| matches!(e, ExecutionEvent::Thinking { content } if content == "All done. Let me summarize what happened.")));
        assert_eq!(events_first.len(), 1);

        // 后续文本行仅累积到 pending_text
        for &line in &text_lines[1..] {
            assert!(ext.extract(line).is_empty(), "line '{}' should produce no events", line);
        }

        // 最后一行：文本末尾无换行直接接 [tokens] 结构化标记
        let events = ext.extract(
            "任务经历了 **pending → in_progress → completed** 三个状态，中间间隔了 1 秒的等待，符合你的要求。[tokens] prompt=7105 completion=202 cached=6912 (97% hit)"
        );

        assert!(events.len() >= 2, "expected >=2 events, got {}: {:?}", events.len(), events);

        // 验证聚合后的 Assistant 事件（所有累积文本）
        let expected_text = "\
全部完成，执行过程总结：\n\
| 步骤 | 命令/操作 | 结果 |\n\
|------|-----------|------|\n\
| 1️⃣ | `date` | `Tue Jun 30 16:21:03 CST 2026` |\n\
| 2️⃣ | `whoami` | `weibh` |\n\
| 3️⃣ | `todo add` 创建任务 | 任务 #1 创建，状态 `pending` → `[ ]` |\n\
| 4️⃣ | `todo update → in_progress` | 状态切换为 `[>]` |\n\
| 5️⃣ | `sleep 1` | 等待 1 秒 |\n\
| 6️⃣ | `todo update → completed` | 状态切换为 `[x]` ✅ |\n\
任务经历了 **pending → in_progress → completed** 三个状态，中间间隔了 1 秒的等待，符合你的要求。";

        assert!(
            events.iter().any(|e| matches!(e, ExecutionEvent::Result { summary } if summary == expected_text)),
            "aggregated Assistant text mismatch"
        );

        // 验证 Tokens 事件
        assert!(
            events.iter().any(|e| matches!(e, ExecutionEvent::Tokens { input, output, .. } if *input == 7105 && *output == 202)),
            "missing Tokens event"
        );

        // [done] 行
        let done_events = ext.extract("[done] 43.0s tokens=34.65K turns=5 tool_calls=6");
        assert!(
            done_events.iter().any(|e| matches!(e, ExecutionEvent::StepFinish { .. })),
            "missing StepFinish event"
        );
    }
}
