//! Step 协议族 Value 导航分支的共享自由函数（096-W2 治理产物）。
//!
//! zhanlu / mimo / mobilecoder 三家提取器不走结构体反序列化，而是直接对
//! `serde_json::Value` 做导航（历史原因：三家协议字段键名有 camelCase/snake_case 差异，
//! 093-B1 统一结构体时这三家的 Value 路径被保留）。其 tool_use / step_finish / text /
//! reasoning 四个分支存在大段逐字同构代码，本模块把同构部分收敛为自由函数，
//! 各家只保留真实的差异点（id 取法、message_id 键名、StepStart/Finish 的 name 与计数语义）。
//!
//! 行为约束：本模块每个函数都是三家既有实现的逐字提取（含 quirks），
//! 任何「顺手优化」都会破坏事件流等价——修订请先核对三家的原始分支。

use crate::execution_events::event::ExecutionEvent;

/// tool_use 分支：产出 ToolCall，若 `state.output` 存在且非空则同帧补一条 ToolResult。
///
/// 三家逐字同构的部分全部在此：tool 名缺省 "bash"、input 取自 `state.input`（缺省 `{}`）、
/// ToolResult 的 `call_id` 恒为空串（既有 quirk：三家协议行内不回传 call 关联 id）、
/// `is_error` 由 `state.status` 是否为 error/failed 判定。
/// 唯一差异——call id 的取法（part.id / part.callID）由调用方算好后传入。
pub(crate) fn extract_tool_pair(
    part: &serde_json::Value,
    call_id: &str,
) -> Vec<ExecutionEvent> {
    let mut events = Vec::new();
    let tool = part.get("tool").and_then(|v| v.as_str()).unwrap_or("bash");
    let input = part
        .get("state")
        .and_then(|s| s.get("input"))
        .cloned()
        .unwrap_or(serde_json::json!({}));

    events.push(ExecutionEvent::ToolCall {
        id: call_id.to_string(),
        name: tool.to_string(),
        input,
    });

    // 工具结果（如果有 output）
    if let Some(output) = part
        .get("state")
        .and_then(|s| s.get("output"))
        .and_then(|v| v.as_str())
    {
        if !output.is_empty() {
            let is_error = part
                .get("state")
                .and_then(|s| s.get("status"))
                .and_then(|v| v.as_str())
                .map(|s| s == "error" || s == "failed")
                .unwrap_or(false);
            events.push(ExecutionEvent::ToolResult {
                call_id: String::new(),
                output: output.to_string(),
                is_error,
            });
        }
    }
    events
}

/// step_finish 分支的 tokens/cost 提取（三家逐字相同）。
///
/// tokens 缺字段回退 0；cache.read/write 包成 Some（与结构体族 Option 口径对齐）；
/// cost 仅在 > 0 时产出（避免零成本噪音事件）。StepFinish 事件本身不在此——
/// 各家的 name/index 语义不同（"step_{idx}" / "step" / 恒 0），由调用方自行补发。
pub(crate) fn extract_tokens_cost(part: &serde_json::Value) -> Vec<ExecutionEvent> {
    let mut events = Vec::new();
    if let Some(tokens) = part.get("tokens") {
        events.push(ExecutionEvent::Tokens {
            input: tokens.get("input").and_then(|v| v.as_u64()).unwrap_or(0),
            output: tokens.get("output").and_then(|v| v.as_u64()).unwrap_or(0),
            cache_read: tokens.get("cache").and_then(|c| c.get("read")).and_then(|v| v.as_u64()),
            cache_write: tokens.get("cache").and_then(|c| c.get("write")).and_then(|v| v.as_u64()),
        });
    }
    if let Some(cost) = part.get("cost").and_then(|v| v.as_f64()) {
        if cost > 0.0 {
            events.push(ExecutionEvent::Cost { cost_usd: cost });
        }
    }
    events
}

/// text 分支：trim 后非空才产出 Assistant；`message_id_key` 是各家唯一的差异点
/// （zhanlu 不取恒 None / mobilecoder 取 `message_id` / mimo 取 `messageID`）。
pub(crate) fn extract_assistant_text(
    part: &serde_json::Value,
    message_id_key: Option<&str>,
) -> Option<ExecutionEvent> {
    let text = part.get("text").and_then(|v| v.as_str())?.trim();
    if text.is_empty() {
        return None;
    }
    Some(ExecutionEvent::Assistant {
        content: text.to_string(),
        thinking: None,
        message_id: message_id_key
            .and_then(|k| part.get(k))
            .and_then(|v| v.as_str())
            .map(String::from),
    })
}

/// reasoning 分支（zhanlu/mimo 逐字相同；mobilecoder 无此分支）：
/// trim 后非空才产出 Thinking，内容截断 500 字符——思考链可能极长，事件流只保留前缀。
pub(crate) fn extract_thinking(part: &serde_json::Value) -> Option<ExecutionEvent> {
    let text = part.get("text").and_then(|v| v.as_str())?.trim();
    if text.is_empty() {
        return None;
    }
    Some(ExecutionEvent::Thinking {
        content: text.chars().take(500).collect(),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
mod tests {
    use super::*;

    /// extract_tool_pair：无 output 时只产 ToolCall；tool 缺省回退 "bash"，input 缺省为 {}
    #[test]
    fn test_extract_tool_pair_call_only() {
        let part = serde_json::json!({"tool": "bash", "state": {"status": "running", "input": {"command": "ls"}}});
        let events = extract_tool_pair(&part, "c1");
        assert_eq!(events.len(), 1);
        assert!(
            matches!(&events[0], ExecutionEvent::ToolCall { id, name, input }
                if id == "c1" && name == "bash" && input["command"] == "ls")
        );
    }

    /// extract_tool_pair：output 非空时同帧补 ToolResult（call_id 恒空串的 quirk 锁定），
    /// status=error 时 is_error 置位
    #[test]
    fn test_extract_tool_pair_with_error_result() {
        let part = serde_json::json!({"tool": "bash", "state": {"status": "error", "output": "boom"}});
        let events = extract_tool_pair(&part, "c2");
        assert_eq!(events.len(), 2);
        assert!(
            matches!(&events[1], ExecutionEvent::ToolResult { call_id, output, is_error: true }
                if call_id.is_empty() && output == "boom")
        );
    }

    /// extract_tokens_cost：tokens 全字段 + cost>0 双产出；缺省字段回退 0
    #[test]
    fn test_extract_tokens_cost_full() {
        let part = serde_json::json!({"tokens": {"input": 50, "output": 40, "cache": {"read": 3, "write": 2}}, "cost": 0.001});
        let events = extract_tokens_cost(&part);
        assert!(
            matches!(&events[0], ExecutionEvent::Tokens { input: 50, output: 40, cache_read: Some(3), cache_write: Some(2) })
        );
        assert!(matches!(&events[1], ExecutionEvent::Cost { cost_usd } if *cost_usd == 0.001));
    }

    /// extract_tokens_cost：cost=0 不产出 Cost 事件（零成本噪音抑制）
    #[test]
    fn test_extract_tokens_cost_zero_cost_suppressed() {
        let part = serde_json::json!({"tokens": {"input": 1, "output": 1}, "cost": 0.0});
        let events = extract_tokens_cost(&part);
        assert_eq!(events.len(), 1, "仅 Tokens，零 cost 应被抑制");
    }

    /// extract_assistant_text：trim 与空串抑制；message_id 按键名差异取到/缺省 None
    #[test]
    fn test_extract_assistant_text_message_id_key_variants() {
        let part = serde_json::json!({"text": "  hello  ", "messageID": "m1"});
        // 命中键名（mimo 形态）
        let ev = extract_assistant_text(&part, Some("messageID")).expect("assistant");
        assert!(
            matches!(ev, ExecutionEvent::Assistant { content, message_id, .. }
                if content == "hello" && message_id.as_deref() == Some("m1"))
        );
        // None 键（zhanlu 形态）→ message_id 恒 None
        let ev = extract_assistant_text(&part, None).expect("assistant");
        assert!(matches!(ev, ExecutionEvent::Assistant { message_id: None, .. }));
        // 空白文本抑制
        let blank = serde_json::json!({"text": "   "});
        assert!(extract_assistant_text(&blank, None).is_none());
    }

    /// extract_thinking：截断 500 字符与空串抑制
    #[test]
    fn test_extract_thinking_truncates_to_500() {
        let long = "x".repeat(600);
        let part = serde_json::json!({"text": long});
        let ev = extract_thinking(&part).expect("thinking");
        assert!(matches!(ev, ExecutionEvent::Thinking { content } if content.chars().count() == 500));
        let blank = serde_json::json!({"text": " "});
        assert!(extract_thinking(&blank).is_none());
    }
}
