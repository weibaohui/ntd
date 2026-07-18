//! 多 Agent 协作提取器：从执行日志中识别"派生了哪些子 agent"。
//!
//! 与 [`crate::todo_progress`] 平行设计：执行完成时一次性扫描全量日志，
//! 把识别到的子 agent 元数据序列化为 JSON 写入 `execution_records.agent_runs`。
//!
//! 设计取舍（与用户确认的方案一致）：
//! - **只存元数据**（名称/角色/状态/启动时间），不存 prompt/result 原文——
//!   原文已在 `execution_logs` 里，前端按需展示，避免大文本（pi 单条数百 KB）撑爆字段。
//! - **执行器无关**：各执行器提取器已把 spawn 工具调用归一为 `tool_call` 日志
//!   （claudecode→`Agent`、mimo 族→`task`/`actor`、codewhale→`agent`、codex→`spawn_agent`），
//!   这里只按 `tool_name` 识别，与 `todo_progress` 认 `TODO_TOOL_NAMES` 完全同理。
//! - atomcode 等纯文本执行器没有结构化 `tool_call`，末尾做一次纯文本兜底（尽力，不保证准确）。

use crate::models::ParsedLogEntry;
use serde::{Deserialize, Serialize};

/// 子 agent 工具名集合（小写匹配）。新增执行器时在此登记其 spawn 工具名。
const AGENT_TOOL_NAMES: &[&str] = &["agent", "task", "actor", "spawn_agent"];

/// 单个子 agent 的元数据。不含输入输出原文（见模块文档说明）。
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct AgentRun {
    /// 显示名：spawn 入参里的 description / name / title / subject。
    pub name: String,
    /// 角色/类型：subagent_type / agent_type / type / role。拿不到则 None。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// 状态：completed / failed / running。完成态扫描默认 completed（记录已跑完）。
    #[serde(default)]
    pub status: String,
    /// 启动时间（UTC），取 spawn 工具调用那条日志的 timestamp。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
}

/// 大小写不敏感判断是否为"派生子 agent"的工具调用。
fn is_agent_tool_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    AGENT_TOOL_NAMES.iter().any(|&n| n == lower)
}

/// 扫描全量日志，提取子 agent 列表。在执行完成（`persist_completion_record`）时调用。
///
/// `success` 透传记录的最终成败：成功则子 agent 标 `completed`；失败时无法逐个判定，
/// 统一标 `unknown`，避免在失败记录上误显 completed（review 反馈）。
///
/// 优先走结构化路径（tool_call 行）；结构化为空时（如 atomcode 纯文本）退回文本兜底。
pub fn extract_agent_runs(logs: &[ParsedLogEntry], success: bool) -> Vec<AgentRun> {
    let status = status_label(success);
    let structured = collect_structured_agent_runs(logs, status);
    if !structured.is_empty() {
        return structured;
    }
    collect_text_agent_runs(logs, status)
}

/// 完成态子 agent 状态标签：记录成功→completed；失败→unknown（不逐个猜）。
fn status_label(success: bool) -> &'static str {
    if success {
        "completed"
    } else {
        "unknown"
    }
}

/// 结构化路径：遍历所有 tool_call 日志，挑出 spawn 类工具调用并解析成 AgentRun。
fn collect_structured_agent_runs(logs: &[ParsedLogEntry], status: &'static str) -> Vec<AgentRun> {
    logs.iter()
        .filter_map(|e| parse_agent_run_from_entry(e, status))
        .collect()
}

/// 从单条 tool_call 日志解析出一个 AgentRun；非 agent 工具或拿不到名字则返回 None。
fn parse_agent_run_from_entry(entry: &ParsedLogEntry, status: &'static str) -> Option<AgentRun> {
    if entry.log_type != "tool_call" {
        return None;
    }
    // tool_call 行的 tool_name 与 content 都被填成工具名（见 db_adapter::from_event），
    // tool_name 优先，拿不到时退回 content，兼容旧 adapter 路径只填了 content 的情况。
    let tool_name = entry.tool_name.as_deref().unwrap_or(&entry.content);
    if !is_agent_tool_name(tool_name) {
        return None;
    }
    let input_json = entry.tool_input_json.as_deref()?;
    let input: serde_json::Value = serde_json::from_str(input_json).ok()?;
    // mimo 族把真实入参包在 input.operation 里（action/subagent_type/description/prompt），
    // 先尝试下沉一层，找不到 operation 就当顶层平铺（claudecode/codewhale 都是平铺）。
    let src = input.get("operation").unwrap_or(&input);
    // 名字优先取结构化字段；拿不到时从 prompt 文本里抓「名字叫X」
    // （codex 的 spawn_agent 没有独立 name 字段，名字写在 prompt 里）。
    let name = pick_str(src, &["description", "name", "title", "subject"])
        .or_else(|| pick_str(src, &["prompt"]).and_then(|p| extract_named_agents(&p).into_iter().next()))?;
    if name.is_empty() {
        return None;
    }
    Some(AgentRun {
        name,
        role: pick_str(src, &["subagent_type", "agent_type", "type", "role"]),
        status: status.to_string(),
        started_at: Some(entry.timestamp.clone()),
    })
}

/// 在 JSON 对象里按候选 key 顺序取第一个非空白字符串。
/// 跳过空串/纯空白：否则 `description:""` 会命中并阻断后续 prompt 兜底，导致 agent 被误丢弃。
fn pick_str(v: &serde_json::Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|k| {
        let s = v.get(*k)?.as_str()?.trim();
        (!s.is_empty()).then(|| s.to_string())
    })
}

/// 纯文本兜底：仅 atomcode 等无结构化 tool_call 的执行器会走到这里。
///
/// 启发式扫描「名字叫X / 名叫X / 名字是X」并去重。极度依赖输出措辞，不保证准确，
/// 仅为了让纯文本执行器也能在界面上体现"派生了子 agent"。
fn collect_text_agent_runs(logs: &[ParsedLogEntry], status: &'static str) -> Vec<AgentRun> {
    let mut seen = std::collections::HashSet::new();
    let mut runs = Vec::new();
    for entry in logs {
        for name in extract_named_agents(&entry.content) {
            // 同名只保留首次出现，避免一段话里反复提到同一个 agent 重复计数。
            if seen.insert(name.clone()) {
                runs.push(AgentRun {
                    name,
                    role: None,
                    status: status.to_string(),
                    started_at: Some(entry.timestamp.clone()),
                });
            }
        }
    }
    runs
}

/// 从一段文本里抓「名字叫X」类标记后的名字 token。
fn extract_named_agents(text: &str) -> Vec<String> {
    const MARKERS: &[&str] = &["名字叫", "名叫", "名字是"];
    let mut out = Vec::new();
    for marker in MARKERS {
        let mut rest = text;
        while let Some(idx) = rest.find(marker) {
            rest = &rest[idx + marker.len()..];
            // 取标记后第一个 token，遇到空白或常见标点即止。
            let token: String = rest
                .trim_start()
                .chars()
                .take_while(|c| {
                    !c.is_whitespace() && !matches!(c, '，' | ',' | '。' | '.' | '：' | ':')
                })
                .collect();
            if !token.is_empty() {
                out.push(token);
            }
        }
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
mod tests {
    use super::*;

    // 测试默认按「记录成功」走提取（status→completed）；失败场景另测。
    fn extract_test(logs: &[ParsedLogEntry]) -> Vec<AgentRun> {
        extract_agent_runs(logs, true)
    }

    fn tool_call(name: &str, input: &serde_json::Value) -> ParsedLogEntry {
        ParsedLogEntry {
            timestamp: "2026-07-18T03:48:20Z".to_string(),
            log_type: "tool_call".to_string(),
            content: name.to_string(),
            usage: None,
            tool_name: Some(name.to_string()),
            tool_input_json: Some(input.to_string()),
        }
    }

    fn text_entry(content: &str) -> ParsedLogEntry {
        ParsedLogEntry {
            timestamp: "2026-07-18T03:48:20Z".to_string(),
            log_type: "assistant".to_string(),
            content: content.to_string(),
            usage: None,
            tool_name: None,
            tool_input_json: None,
        }
    }

    #[test]
    fn test_claudecode_agent_tool_use() {
        // claudecode：Agent 工具，平铺 description + subagent_type。
        let input = serde_json::json!({
            "description": "张三丰加法计算",
            "subagent_type": "general-purpose",
            "prompt": "计算 8+8"
        });
        let runs = extract_test(&[tool_call("Agent", &input)]);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].name, "张三丰加法计算");
        assert_eq!(runs[0].role.as_deref(), Some("general-purpose"));
        assert_eq!(runs[0].status, "completed");
    }

    #[test]
    fn test_mimo_task_tool_nested_operation() {
        // mimo 族：真实入参包在 operation 里。
        let input = serde_json::json!({
            "operation": {
                "action": "run",
                "subagent_type": "general",
                "description": "张三丰：加法计算 8+8",
                "prompt": "你是张三丰..."
            }
        });
        let runs = extract_test(&[tool_call("task", &input)]);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].name, "张三丰：加法计算 8+8");
        assert_eq!(runs[0].role.as_deref(), Some("general"));
    }

    #[test]
    fn test_codewhale_agent_tool_with_type() {
        // codewhale：agent 工具，name + type 平铺。
        let input = serde_json::json!({
            "action": "start",
            "name": "zhangsanfeng",
            "type": "general",
            "prompt": "你是张三丰..."
        });
        let runs = extract_test(&[tool_call("agent", &input)]);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].name, "zhangsanfeng");
        assert_eq!(runs[0].role.as_deref(), Some("general"));
    }

    #[test]
    fn test_multiple_agents_preserve_order() {
        // 两个 agent 并行 spawn，顺序应保持（先张三丰后李雷）。
        let a = serde_json::json!({"description": "张三丰", "subagent_type": "general"});
        let b = serde_json::json!({"description": "李雷", "subagent_type": "general"});
        let runs = extract_test(&[tool_call("Agent", &a), tool_call("Agent", &b)]);
        assert_eq!(runs.iter().map(|r| r.name.clone()).collect::<Vec<_>>(), vec!["张三丰", "李雷"]);
    }

    #[test]
    fn test_codex_spawn_agent_name_from_prompt() {
        // codex：spawn_agent 没有独立 name 字段，名字嵌在 prompt 文本里。
        let input = serde_json::json!({
            "prompt": "你的名字叫张三丰。你的角色是加法计算专家。请只计算 8+8。",
            "receiver_thread_ids": ["019f735d-2365"]
        });
        let runs = extract_test(&[tool_call("spawn_agent", &input)]);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].name, "张三丰");
    }

    #[test]
    fn test_non_agent_tool_ignored() {
        // 普通工具调用（bash/read/todowrite）不应被误识别为子 agent。
        let input = serde_json::json!({"command": "ls -la"});
        assert!(extract_test(&[tool_call("bash", &input)]).is_empty());
    }

    #[test]
    fn test_empty_logs() {
        assert!(extract_test(&[]).is_empty());
    }

    #[test]
    fn test_failed_record_marks_agents_unknown() {
        // 记录失败时无法逐个判定子 agent 成败，统一标 unknown，避免误显 completed（review 反馈）。
        let input = serde_json::json!({"description": "张三丰", "subagent_type": "general"});
        let runs = extract_agent_runs(&[tool_call("Agent", &input)], false);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].status, "unknown");
    }

    #[test]
    fn test_agent_without_name_dropped() {
        // 拿不到名字（description/name 都缺）的 spawn 不算一个可展示的 agent。
        let input = serde_json::json!({"prompt": "做点事"});
        assert!(extract_test(&[tool_call("Agent", &input)]).is_empty());
    }

    #[test]
    fn test_atomcode_text_fallback() {
        // 纯文本执行器（atomcode）：结构化为空时走文本兜底，抓「名字叫X」。
        let text = "Agent 1 名字叫张三丰，是加法专家；Agent 2 名字叫李雷。";
        let runs = extract_test(&[text_entry(text)]);
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].name, "张三丰");
        assert_eq!(runs[1].name, "李雷");
    }

    #[test]
    fn test_text_fallback_not_triggered_when_structured_exists() {
        // 有结构化 agent 时不走文本兜底，避免把正文里提到的名字误当 agent。
        let input = serde_json::json!({"description": "张三丰"});
        let text = "正文里提到名字叫李雷，但这是普通文本。";
        let runs = extract_test(&[tool_call("Agent", &input), text_entry(text)]);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].name, "张三丰");
    }
}
