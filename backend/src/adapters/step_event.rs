//! Step 协议族的统一事件模型（093-B1 提炼基类专项）。
//!
//! ## 背景
//! kilo / opencode / zhanlu / mimo 四个执行器输出同一套 JSONL 协议
//! （step_start / tool_use / text / step_finish ...），此前各持有一份
//! 逐字相同的 serde 结构体（kilo_event.rs 头部注释自述「只把类型名重命名」）。
//! 本文件是唯一事实源；旧事件模块降级为别名壳，保持既有引用路径不抖。
//!
//! ## 与旧四份结构的差异（有意的语义对齐）
//! 1. mimo 的 camelCase 键（`callID`/`messageID`/`sessionID`）用 serde `alias`
//!    兼容——反序列化两种键名都收，序列化恒输出 snake_case（与 mimo 现状一致）。
//! 2. `snapshot` 字段来自 mimo；其余三家协议不产出该键，反序列化恒 None，无影响。
//! 3. `to_full_json` 采用 mimo 的防覆盖版本：extra 中与 command/description 同名的键
//!    不再覆盖结构化字段（旧 kilo 系版本允许覆盖，属意外场景，统一为更安全的语义）。
//! 4. ToolState/ToolInput 增加 `Serialize` derive（mimo 的 tool_use 需序列化整个 state；
//!    kilo 系不走此路径，derive 无害）。

use std::collections::HashMap;
use serde::{Deserialize, Serialize};

/// Step 协议 agent 事件（兼容下划线式与连字符式事件名，由各 flavor 自行匹配）
#[derive(Debug, Clone, Deserialize)]
pub struct StepAgentEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    #[serde(default)]
    pub timestamp: Option<u64>,
    /// 顶层 session id：kilo 系 JSON 键恒为 `sessionID`。
    /// 用 alias 而非 rename（CodeRabbit #1008 评审）：与下方 StepAgentPart.session_id
    /// 的容错范围对齐——两种键名都收，避免某 flavor 未来顶层发 snake_case 时
    /// 字段静默落 None（反序列化不会报错，最难排查的一类漂移）。
    #[serde(default, alias = "sessionID")]
    pub session_id: Option<String>,
    #[serde(default)]
    pub part: Option<StepAgentPart>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StepAgentPart {
    #[serde(rename = "type")]
    pub part_type: Option<String>,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub tool: Option<String>,
    /// kilo 系 JSON 用 `call_id`，mimo 用 `callID`；alias 双收
    #[serde(default, alias = "callID")]
    pub call_id: Option<String>,
    #[serde(default)]
    pub state: Option<StepAgentToolState>,
    #[serde(default, alias = "messageID")]
    pub message_id: Option<String>,
    #[serde(default, alias = "sessionID")]
    pub session_id: Option<String>,
    #[serde(default)]
    pub tokens: Option<StepAgentTokens>,
    #[serde(default)]
    pub cost: Option<f64>,
    #[serde(default)]
    pub reason: Option<String>,
    /// mimo 专有快照字段；其余三家协议不产出，反序列化恒 None
    #[serde(default)]
    pub snapshot: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StepAgentToolState {
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub input: Option<StepAgentToolInput>,
    #[serde(default)]
    pub output: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StepAgentToolInput {
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

impl StepAgentToolInput {
    /// 序列化为完整 JSON Value。
    /// 结构化字段（command/description）优先级最高；extra 中同名键跳过，
    /// 防止 extra 覆盖核心语义（采用 mimo 旧版的防覆盖语义）。
    /// 096-W2：Value 形态独立成方法（kilo/opencode 提取器的 ToolCall input 需要 Value），
    /// `to_full_json` 退化为本方法的字符串化委托。
    pub fn to_full_json_value(&self) -> serde_json::Value {
        let mut map = serde_json::Map::new();
        if let Some(ref cmd) = self.command {
            map.insert("command".into(), serde_json::Value::String(cmd.clone()));
        }
        if let Some(ref desc) = self.description {
            map.insert("description".into(), serde_json::Value::String(desc.clone()));
        }
        for (k, v) in &self.extra {
            if k == "command" || k == "description" {
                continue;
            }
            map.insert(k.clone(), v.clone());
        }
        serde_json::Value::Object(map)
    }

    /// 序列化为完整 JSON 字符串。
    /// 结构化字段（command/description）优先级最高；extra 中同名键跳过，
    /// 防止 extra 覆盖核心语义（采用 mimo 旧版的防覆盖语义）。
    pub fn to_full_json(&self) -> String {
        serde_json::to_string(&self.to_full_json_value()).unwrap_or_default()
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct StepAgentTokens {
    // total/input/output 必填是刻意设计：与合并前四家旧事件结构体逐字对齐
    // （zhanlu/kilo/opencode/mimo 的 Tokens 均如此）。若放宽为 default=0，
    // 缺 usage 的 step_finish 将从「解析失败丢事件」变成「成功置位 + 零值统计」，
    // 破坏 NTD-012 的成败判定语义——该容错方向须在确认真实协议形态后单开变更。
    pub total: u64,
    pub input: u64,
    pub output: u64,
    #[serde(default)]
    pub reasoning: u64,
    #[serde(default)]
    pub cache: StepAgentCacheTokens,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct StepAgentCacheTokens {
    #[serde(default)]
    pub read: u64,
    #[serde(default)]
    pub write: u64,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
mod tests {
    use super::*;

    /// 基础反序列化：下划线式事件名 + 缺省字段（timestamp/part 缺省为 None）
    #[test]
    fn test_step_event_basic_underscore_form() {
        let json = r#"{"type":"step_start","timestamp":1700000000000}"#;
        let event: StepAgentEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event.event_type, "step_start");
        assert_eq!(event.timestamp, Some(1700000000000u64));
        assert!(event.session_id.is_none());
        assert!(event.part.is_none());
    }

    /// 连字符式事件名 + sessionID 键（kilo 系真实形态）
    #[test]
    fn test_step_event_hyphenated_with_session_id() {
        let json = r#"{"type":"step-start","timestamp":1777471473403,"sessionID":"ses_abc123"}"#;
        let event: StepAgentEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event.event_type, "step-start");
        assert_eq!(event.session_id, Some("ses_abc123".to_string()));
    }

    /// 顶层 session_id 双键名容错（CodeRabbit #1008 评审项）：
    /// alias 后 snake_case `session_id` 与 camelCase `sessionID` 均应落入同一字段
    #[test]
    fn test_step_event_top_level_session_id_accepts_snake_case() {
        let snake = r#"{"type":"step_start","session_id":"ses_snake"}"#;
        let camel = r#"{"type":"step_start","sessionID":"ses_camel"}"#;
        let snake_ev: StepAgentEvent = serde_json::from_str(snake).unwrap();
        let camel_ev: StepAgentEvent = serde_json::from_str(camel).unwrap();
        assert_eq!(snake_ev.session_id, Some("ses_snake".to_string()), "snake_case 键应经 alias 落入");
        assert_eq!(camel_ev.session_id, Some("ses_camel".to_string()), "camelCase 键保持兼容");
    }

    /// step_finish part 的 tokens/cost 完整解析（usage 提取链路的上游）
    #[test]
    fn test_step_event_step_finish_tokens() {
        let json = r#"{"type":"step-finish","timestamp":1700000000002,"part":{"type":"step-finish","reason":"stop","tokens":{"total":200,"input":150,"output":50,"reasoning":0,"cache":{"read":10,"write":5}},"cost":0.0025}}"#;
        let event: StepAgentEvent = serde_json::from_str(json).unwrap();
        let part = event.part.unwrap();
        assert_eq!(part.reason, Some("stop".to_string()));
        assert_eq!(part.cost, Some(0.0025));
        let tokens = part.tokens.unwrap();
        assert_eq!((tokens.total, tokens.input, tokens.output), (200, 150, 50));
        assert_eq!((tokens.cache.read, tokens.cache.write), (10, 5));
    }

    /// mimo 形态：camelCase 键（callID/messageID/sessionID）经 alias 落到 snake_case 字段
    #[test]
    fn test_step_event_mimo_camel_case_aliases() {
        let json = r#"{"type":"tool_use","part":{"type":"tool_use","tool":"bash","callID":"c1","messageID":"m1","sessionID":"s1","snapshot":"snap"}}"#;
        let event: StepAgentEvent = serde_json::from_str(json).unwrap();
        let part = event.part.unwrap();
        assert_eq!(part.call_id, Some("c1".to_string()), "callID 应经 alias 落入 call_id");
        assert_eq!(part.message_id, Some("m1".to_string()));
        assert_eq!(part.session_id, Some("s1".to_string()));
        assert_eq!(part.snapshot, Some("snap".to_string()), "mimo 专有 snapshot 字段");
    }

    /// kilo 系形态：snake_case 键（call_id）不受 alias 影响
    #[test]
    fn test_step_event_snake_case_keys_still_work() {
        let json = r#"{"type":"tool_use","part":{"type":"tool_use","call_id":"c9","session_id":"s9"}}"#;
        let event: StepAgentEvent = serde_json::from_str(json).unwrap();
        let part = event.part.unwrap();
        assert_eq!(part.call_id, Some("c9".to_string()));
        assert_eq!(part.session_id, Some("s9".to_string()));
        assert!(part.snapshot.is_none(), "非 mimo 协议 snapshot 恒 None");
    }

    /// tool_use part 的 state 嵌套解析（status/input/output）
    #[test]
    fn test_step_event_tool_state_nested() {
        let json = r#"{"type":"tool-use","part":{"type":"tool_use","tool":"bash","state":{"status":"running","input":{"description":"list files","command":"ls -la"},"output":null}}}"#;
        let event: StepAgentEvent = serde_json::from_str(json).unwrap();
        let state = event.part.unwrap().state.unwrap();
        assert_eq!(state.status, Some("running".to_string()));
        let input = state.input.unwrap();
        assert_eq!(input.description, Some("list files".to_string()));
        assert_eq!(input.command, Some("ls -la".to_string()));
    }

    /// to_full_json：结构化字段优先，extra 同名键不覆盖（防覆盖语义对齐）
    #[test]
    fn test_to_full_json_extra_does_not_override_core_keys() {
        let mut extra = HashMap::new();
        extra.insert("command".to_string(), serde_json::Value::String("evil".to_string()));
        extra.insert("cwd".to_string(), serde_json::Value::String("/tmp".to_string()));
        let input = StepAgentToolInput {
            command: Some("ls".to_string()),
            description: None,
            extra,
        };
        let v: serde_json::Value = serde_json::from_str(&input.to_full_json()).unwrap();
        assert_eq!(v["command"], "ls", "extra 中的 command 不得覆盖结构化字段");
        assert_eq!(v["cwd"], "/tmp", "extra 的非同名键应透传");
    }

    /// ToolState 可序列化（mimo tool_use 的 state_json 载荷依赖）
    #[test]
    fn test_tool_state_serializes_to_json() {
        let state = StepAgentToolState {
            status: Some("success".to_string()),
            input: None,
            output: Some("done".to_string()),
        };
        let v: serde_json::Value = serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
        assert_eq!(v["status"], "success");
        assert_eq!(v["output"], "done");
    }
}
