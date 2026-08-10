//! Step 协议族统一执行器（093-B1 提炼基类专项）。
//!
//! kilo / opencode / zhanlu / mimo 四个执行器共享同一 JSONL 协议与命令行形态，
//! 此前是四份 77%~83% 逐字相同的复制粘贴文件（共 1840 行）。本文件是唯一实现：
//! 共享逻辑一份，行为差异全部收敛到 [`StepProtocolFlavor`] 的 6 个查询方法上——
//! 协议调整从「改 4 处」变为「改 1 处」。
//!
//! 差异矩阵（与设计文档 §1.2 逐格对应）：
//!
//! | 行为点 | kilo/opencode/zhanlu | mimo |
//! |--------|---------------------|------|
//! | 事件名风格 | 下划线+连字符双兼容 | 仅下划线 |
//! | reasoning 事件 | 无 | 有（→thinking，截 500 字符） |
//! | tool_use JSON 载荷 | 仅 input.to_full_json() | 整个 state 序列化 |
//! | resume 无 session_id | 不加参数 | 降级 `-c` |
//! | get_model | base.model | 恒 None |

use std::sync::Arc;
use parking_lot::Mutex;

use super::helpers;
use super::step_event::{StepAgentEvent, StepAgentPart};
use super::{BaseExecutor, CodeExecutor, ExecutorType, ParsedLogEntry};
use crate::models::ExecutionUsage;
use crate::models::utc_timestamp;

/// Step 协议族的执行器身份标识——行为差异的唯一载体。
/// 每个变体的方法回答「我与协议族标准行为的哪一格不同」，见模块头差异矩阵。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StepProtocolFlavor {
    Kilo,
    Opencode,
    Zhanlu,
    Mimo,
}

impl StepProtocolFlavor {
    /// 注册表与 ExecutorType 枚举的对应关系（各构造器由此决定自身类型）。
    fn executor_type(self) -> ExecutorType {
        match self {
            Self::Kilo => ExecutorType::Kilo,
            Self::Opencode => ExecutorType::Opencode,
            Self::Zhanlu => ExecutorType::Zhanlu,
            Self::Mimo => ExecutorType::Mimo,
        }
    }

    /// kilo 系同时接受 `step_start` 与 `step-start` 两种事件名（协议演进遗留），
    /// mimo 只产出下划线式——接受多余形态会让 mimo 对连字符行误判，故按 flavor 区分。
    fn accepts_hyphenated_events(self) -> bool {
        !matches!(self, Self::Mimo)
    }

    /// 仅 mimo 协议有 reasoning 事件（映射为 thinking 日志，截 500 字符防爆版式）。
    fn has_reasoning_event(self) -> bool {
        matches!(self, Self::Mimo)
    }

    /// tool_use 的 tool_input_json 载荷形态：
    /// mimo 序列化整个 state（前端 extractAgentCommands 依赖 state.status 判定成败），
    /// kilo 系只序列化 input（历史形态，前端对该三家的解析路径不同）。
    fn serializes_full_tool_state(self) -> bool {
        matches!(self, Self::Mimo)
    }

    /// resume 且未提供 session_id 时：mimo 降级 `-c`（续接最近会话），
    /// kilo 系不加参数（启动新会话）。语义差异来自各家 CLI 的参数设计，不可统一。
    fn resume_falls_back_to_dash_c(self) -> bool {
        matches!(self, Self::Mimo)
    }

    /// mimo 的 JSON 输出不含模型名（get_model 恒 None）；
    /// kilo 系经 `set_exec_model` 注入后由 `base.model` 回报。
    fn reports_model(self) -> bool {
        !matches!(self, Self::Mimo)
    }
}

/// Step 协议族统一执行器。
///
/// 状态三件套：`base`（路径+模型+usage，共享自 BaseExecutor）、
/// `has_successful_finish`（「非零退出码但有 step_finish 即算成功」语义标记，
/// 由 step_start 重置 / step_finish 置位，check_success 读取）、
/// `session_id`（JSON 事件缓存，支持跨行回退与 resume 回写 DB）。
/// `Arc<Mutex<..>>` 使 derive(Clone) 后克隆体共享内部状态——执行期读写分离
/// （parse 写 / check_success 读）依赖这一点。
#[derive(Clone)]
pub struct StepProtocolExecutor {
    base: BaseExecutor,
    has_successful_finish: Arc<Mutex<bool>>,
    session_id: Arc<Mutex<Option<String>>>,
    flavor: StepProtocolFlavor,
}

impl StepProtocolExecutor {
    /// 各执行器的命名构造入口（注册表调用点保持语义直白）。
    pub fn kilo(path: String) -> Self {
        Self::new(path, StepProtocolFlavor::Kilo)
    }

    pub fn opencode(path: String) -> Self {
        Self::new(path, StepProtocolFlavor::Opencode)
    }

    pub fn zhanlu(path: String) -> Self {
        Self::new(path, StepProtocolFlavor::Zhanlu)
    }

    pub fn mimo(path: String) -> Self {
        Self::new(path, StepProtocolFlavor::Mimo)
    }

    fn new(path: String, flavor: StepProtocolFlavor) -> Self {
        Self {
            base: BaseExecutor::new(path),
            has_successful_finish: Arc::new(Mutex::new(false)),
            session_id: Arc::new(Mutex::new(None)),
            flavor,
        }
    }

    /// 更新 session_id 缓存（extract_session_id 和 parse_output_line 共用）。
    /// Option 入参是为了把「事件里没有 sid 字段」与「有 sid」两种情况的调用点合并。
    #[allow(clippy::needless_pass_by_value)]
    fn update_session_id_cache(&self, sid: Option<String>) {
        if let Some(ref s) = sid {
            *self.session_id.lock() = Some(s.clone());
        }
    }

    /// 把协议时间戳（毫秒）转换为 ISO 字符串；缺失时回退到当前 UTC。
    fn resolve_timestamp(ts: Option<u64>) -> String {
        ts.and_then(|ts| chrono::DateTime::from_timestamp_millis(ts as i64))
            .map(|dt| dt.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string())
            .unwrap_or_else(utc_timestamp)
    }

    /// step_start：重置 has_successful_finish（新一轮 step 的成功标记清零）。
    fn handle_step_start(&self, timestamp: &str) -> Option<ParsedLogEntry> {
        *self.has_successful_finish.lock() = false;
        Some(helpers::with_timestamp(helpers::entry("step_start", "Step started"), timestamp))
    }

    /// tool_use：bash 工具显示 description + output，其它工具显示 tool + description。
    fn handle_tool_use(&self, part: &StepAgentPart, timestamp: &str) -> Option<ParsedLogEntry> {
        let tool = part.tool.clone().unwrap_or_default();
        let status = part.state.as_ref().and_then(|s| s.status.clone()).unwrap_or_default();
        // bash 工具显示描述文字而非原始命令，更适合用户阅读
        let description = part.state.as_ref()
            .and_then(|s| s.input.as_ref()?.description.clone())
            .unwrap_or_default();
        // tool_input_json 载荷形态是 flavor 差异点（见 StepProtocolFlavor::serializes_full_tool_state）
        let tool_json = if self.flavor.serializes_full_tool_state() {
            part.state.as_ref().map(|s| serde_json::to_string(s).unwrap_or_default())
        } else {
            part.state.as_ref().and_then(|s| s.input.as_ref()).map(|i| i.to_full_json())
        };

        let content = if tool == "bash" {
            // bash 特殊渲染：把命令输出也带上
            match &part.state.as_ref().and_then(|s| s.output.clone()) {
                Some(output) => format!("[{}] {}: {}", status, description, output),
                None => format!("[{}] {}", status, description),
            }
        } else {
            format!("[{}] Tool: {} - {}", status, tool, description)
        };

        Some(helpers::with_timestamp(
            helpers::entry_with_optional_tool("tool", content, Some(tool), tool_json),
            timestamp,
        ))
    }

    /// text：空文本返回 None（不产生空日志条目），否则返回 text 日志。
    fn handle_text(&self, part: &StepAgentPart, timestamp: &str) -> Option<ParsedLogEntry> {
        let text = part.text.clone().unwrap_or_default();
        if text.is_empty() {
            return None;
        }
        Some(helpers::with_timestamp(helpers::text_entry(text), timestamp))
    }

    /// reasoning（仅 mimo flavor 路由到此）：思考过程截 500 字符，避免占用过多显示空间。
    fn handle_reasoning(&self, part: &StepAgentPart, timestamp: &str) -> Option<ParsedLogEntry> {
        let text = part.text.clone().unwrap_or_default();
        if text.is_empty() {
            return None;
        }
        let trimmed: String = text.chars().take(500).collect();
        Some(helpers::with_timestamp(helpers::entry("thinking", trimmed), timestamp))
    }

    /// step_finish：标记 has_successful_finish，从 tokens 提取 usage。
    fn handle_step_finish(&self, event: &StepAgentEvent, timestamp: &str) -> Option<ParsedLogEntry> {
        // 标记执行成功：即使退出码非零，只要收到 step_finish 事件即表示正常完成
        *self.has_successful_finish.lock() = true;
        // 从 step_finish 的 tokens 字段提取 usage
        let usage = event.part.as_ref().and_then(|part| {
            part.tokens.as_ref().map(|tokens| ExecutionUsage {
                input_tokens: tokens.input,
                output_tokens: tokens.output,
                cache_read_input_tokens: if tokens.cache.read > 0 { Some(tokens.cache.read) } else { None },
                cache_creation_input_tokens: if tokens.cache.write > 0 { Some(tokens.cache.write) } else { None },
                total_cost_usd: part.cost,
                duration_ms: None,
            })
        });
        Some(helpers::with_timestamp(helpers::entry_with_usage("step_finish", "Step finished", usage), timestamp))
    }
}

impl CodeExecutor for StepProtocolExecutor {
    fn executor_type(&self) -> ExecutorType {
        self.flavor.executor_type()
    }

    fn executable_path(&self) -> &str {
        &self.base.path
    }

    fn command_args(&self, message: &str) -> Vec<String> {
        // 四家 CLI 的基础参数形态完全一致：run --format json --dangerously-skip-permissions。
        // --dangerously-skip-permissions 默认启用：ntd 的设计目标是无人值守自动化执行，
        // 用户选择本族执行器即表示接受自动批准权限的行为。
        vec![
            "run".to_string(),
            "--format".to_string(),
            "json".to_string(),
            "--dangerously-skip-permissions".to_string(),
            message.to_string(),
        ]
    }

    fn command_args_with_session(&self, message: &str, session_id: Option<&str>, is_resume: bool) -> Vec<String> {
        let mut args = vec![
            "run".to_string(),
            "--format".to_string(),
            "json".to_string(),
        ];
        // 注入模型（本族 CLI 均接受 -m provider/model）
        if let Some(m) = self.base.model.lock().clone() {
            args.push("-m".to_string());
            args.push(m);
        }
        if is_resume {
            if let Some(sid) = session_id {
                // 恢复模式：优先用 -s 精确恢复指定 session
                args.push("-s".to_string());
                args.push(sid.to_string());
            } else if self.flavor.resume_falls_back_to_dash_c() {
                // mimo 专有：未提供 session_id 时用 -c 续接最近会话
                args.push("-c".to_string());
            }
        }
        args.push("--dangerously-skip-permissions".to_string());
        args.push(message.to_string());
        args
    }

    fn supports_resume(&self) -> bool {
        true
    }

    /// 执行前注入期望模型，写入 base.model，供 command_args_with_session 拼 -m。
    fn set_exec_model(&self, model: Option<String>) {
        *self.base.model.lock() = model;
    }

    /// 从事件中提取 session_id（优先顶层字段，再 part 内字段），提取后缓存到 executor 状态。
    fn extract_session_id(&self, line: &str) -> Option<String> {
        let event: StepAgentEvent = serde_json::from_str(line).ok()?;
        let sid = event.session_id.or_else(|| event.part.as_ref()?.session_id.clone());
        self.update_session_id_cache(sid.clone());
        sid.or_else(|| self.session_id.lock().clone())
    }

    fn get_session_id(&self) -> Option<String> {
        self.session_id.lock().clone()
    }

    fn parse_output_line(&self, line: &str) -> Option<ParsedLogEntry> {
        let event: StepAgentEvent = serde_json::from_str(line).ok()?;
        // 缓存 session_id：优先取事件顶层字段，再取 part 内字段
        let sid = event.session_id.clone().or_else(|| event.part.as_ref()?.session_id.clone());
        self.update_session_id_cache(sid);
        let timestamp = Self::resolve_timestamp(event.timestamp);

        // 事件名归一化：kilo 系把连字符式折叠为下划线式统一匹配；
        // mimo flavor 不归一化——其协议不产出连字符名，未知名自然落入 _ 分支
        let normalized = if self.flavor.accepts_hyphenated_events() {
            event.event_type.replace('-', "_")
        } else {
            event.event_type.clone()
        };
        match normalized.as_str() {
            "step_start" => self.handle_step_start(&timestamp),
            "tool_use" => self.handle_tool_use(event.part.as_ref()?, &timestamp),
            "text" => self.handle_text(event.part.as_ref()?, &timestamp),
            // reasoning 仅 mimo flavor 会到达（kilo 系协议无此事件名）
            "reasoning" if self.flavor.has_reasoning_event() => {
                self.handle_reasoning(event.part.as_ref()?, &timestamp)
            }
            "step_finish" => self.handle_step_finish(&event, &timestamp),
            _ => None,
        }
    }

    fn get_final_result(&self, logs: &[ParsedLogEntry]) -> Option<String> {
        // 通用 fallback 链（result → text → stderr）覆盖本族全部场景：
        // pipeline.finalize() 会把最后一个 Assistant 提升为 Result，日志侧始终有结论可提取。
        super::default_final_result_with_think_stripping(logs)
    }

    fn get_model(&self) -> Option<String> {
        // mimo 的 JSON 输出不含模型名（恒 None）；kilo 系由 set_exec_model 注入后回报
        if self.flavor.reports_model() {
            self.base.model.lock().clone()
        } else {
            None
        }
    }

    /// NTD-012：pipeline 路径下同步 step 生命周期标记。
    /// EventPipeline 命中 step_start/step_finish 后旧 `parse_output_line` 被跳过，
    /// 本钩子把已解析事件回授给执行器，保持 `check_success` 的非零退出码容忍语义。
    /// 语义与旧 handler 逐字对齐：step_start 重置、step_finish 置位。
    fn on_pipeline_event(&self, event: &crate::execution_events::ExecutionEvent) {
        use crate::execution_events::ExecutionEvent as E;
        match event {
            E::StepStart { .. } => *self.has_successful_finish.lock() = false,
            E::StepFinish { .. } => *self.has_successful_finish.lock() = true,
            _ => {}
        }
    }

    fn check_success(&self, exit_code: i32) -> bool {
        if exit_code == 0 {
            return true;
        }
        // 本族 CLI 可能返回非零退出码（如 144 / 模型超时）即使执行成功，
        // 通过 step_finish 事件是否到达判断真实成败。
        *self.has_successful_finish.lock()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
mod tests {
    use super::*;
    use crate::models::ParsedLogEntry;

    // ── 通用解析行为（四 flavor 共享，用 kilo/opencode/zhanlu/mimo 各跑一遍关键路径）──

    /// 各 flavor 的基础解析：step_start（下划线式四家都接受）
    #[test]
    fn test_parse_step_start_all_flavors() {
        for exec in [
            StepProtocolExecutor::kilo("kilo".into()),
            StepProtocolExecutor::opencode("opencode".into()),
            StepProtocolExecutor::zhanlu("zl".into()),
            StepProtocolExecutor::mimo("mimo".into()),
        ] {
            let entry = exec
                .parse_output_line(r#"{"type":"step_start","timestamp":1700000000000}"#)
                .unwrap();
            assert_eq!(entry.log_type, "step_start");
            assert_eq!(entry.content, "Step started");
        }
    }

    /// 连字符式事件名：kilo 系接受，mimo 拒绝（差异矩阵第 1 格）
    #[test]
    fn test_parse_output_line_hyphenated_events_matrix() {
        let line = r#"{"type":"step-start","timestamp":1700000000000,"sessionID":"ses_x"}"#;
        for exec in [
            StepProtocolExecutor::kilo("kilo".into()),
            StepProtocolExecutor::opencode("opencode".into()),
            StepProtocolExecutor::zhanlu("zl".into()),
        ] {
            assert!(exec.parse_output_line(line).is_some(), "kilo 系应接受连字符事件名");
        }
        assert!(
            StepProtocolExecutor::mimo("mimo".into()).parse_output_line(line).is_none(),
            "mimo 不应接受连字符事件名"
        );
    }

    /// tool_use（bash）：content 渲染四家一致
    #[test]
    fn test_parse_tool_use_bash_content() {
        let line = r#"{"type":"tool_use","timestamp":1700000000000,"part":{"type":"tool_use","tool":"bash","state":{"status":"success","input":{"description":"list files"},"output":"file.txt"}}}"#;
        let exec = StepProtocolExecutor::kilo("kilo".into());
        let entry = exec.parse_output_line(line).unwrap();
        assert_eq!(entry.log_type, "tool");
        assert!(entry.content.contains("success"));
        assert!(entry.content.contains("list files"));
        assert!(entry.content.contains("file.txt"));
    }

    /// tool_use 的 JSON 载荷形态：kilo 系=仅 input；mimo=整个 state（差异矩阵第 3 格）
    #[test]
    fn test_tool_use_json_payload_differs_by_flavor() {
        let line = r#"{"type":"tool_use","timestamp":1700000000000,"part":{"type":"tool_use","tool":"bash","state":{"status":"success","input":{"description":"d"},"output":"o"}}}"#;
        let kilo_entry = StepProtocolExecutor::kilo("kilo".into()).parse_output_line(line).unwrap();
        let kilo_json: serde_json::Value =
            serde_json::from_str(kilo_entry.tool_input_json.as_deref().unwrap()).unwrap();
        assert!(kilo_json.get("status").is_none(), "kilo 系载荷不含 state 外层字段");
        assert_eq!(kilo_json["description"], "d");

        let mimo_entry = StepProtocolExecutor::mimo("mimo".into()).parse_output_line(line).unwrap();
        let mimo_json: serde_json::Value =
            serde_json::from_str(mimo_entry.tool_input_json.as_deref().unwrap()).unwrap();
        assert_eq!(mimo_json["status"], "success", "mimo 载荷是整个 state（前端依赖 status 判定成败）");
    }

    /// reasoning：仅 mimo 路由到 thinking 日志；kilo 系视为未知事件（差异矩阵第 2 格）
    #[test]
    fn test_parse_output_line_reasoning_only_for_mimo() {
        let line = r#"{"type":"reasoning","timestamp":1700000000000,"part":{"type":"reasoning","text":"thinking..."}}"#;
        let mimo_entry = StepProtocolExecutor::mimo("mimo".into()).parse_output_line(line).unwrap();
        assert_eq!(mimo_entry.log_type, "thinking");
        assert_eq!(mimo_entry.content, "thinking...");
        assert!(
            StepProtocolExecutor::kilo("kilo".into()).parse_output_line(line).is_none(),
            "kilo 系不识别 reasoning 事件"
        );
    }

    /// reasoning 超长截断（500 字符上限，mimo）
    #[test]
    fn test_reasoning_truncated_at_500_chars() {
        let long_text = "思".repeat(600);
        let line = format!(
            r#"{{"type":"reasoning","part":{{"type":"reasoning","text":"{long_text}"}}}}"#
        );
        let entry = StepProtocolExecutor::mimo("mimo".into()).parse_output_line(&line).unwrap();
        assert_eq!(entry.content.chars().count(), 500, "reasoning 应截断到 500 字符");
    }

    /// text：空文本不产生日志条目
    #[test]
    fn test_parse_empty_text_returns_none() {
        let exec = StepProtocolExecutor::kilo("kilo".into());
        let line = r#"{"type":"text","timestamp":1700000000000,"part":{"type":"text","text":""}}"#;
        assert!(exec.parse_output_line(line).is_none());
    }

    /// 未知事件类型与非 JSON 行均返回 None（由调用方决定是否跳过）
    #[test]
    fn test_parse_unknown_and_invalid_lines() {
        let exec = StepProtocolExecutor::opencode("opencode".into());
        assert!(exec.parse_output_line(r#"{"type":"unknown","timestamp":1}"#).is_none());
        assert!(exec.parse_output_line("not json").is_none());
    }

    /// step_finish 置位成功标记 + usage 提取（cache>0 才进 usage）
    #[test]
    fn test_step_finish_sets_flag_and_extracts_usage() {
        let exec = StepProtocolExecutor::zhanlu("zl".into());
        assert!(!exec.check_success(144), "未收到 step_finish 前非零退出应判失败");
        let line = r#"{"type":"step-finish","timestamp":1700000000000,"part":{"type":"step-finish","reason":"stop","tokens":{"total":100,"input":50,"output":50,"cache":{"read":10,"write":5}},"cost":0.001}}"#;
        let entry = exec.parse_output_line(line).unwrap();
        assert_eq!(entry.log_type, "step_finish");
        let usage = entry.usage.expect("step_finish 应携带 usage");
        assert_eq!(usage.input_tokens, 50);
        assert_eq!(usage.cache_read_input_tokens, Some(10));
        assert!(exec.check_success(144), "收到 step_finish 后非零退出应判成功");
    }

    /// step_start 重置成功标记（新一轮 step 清零）
    #[test]
    fn test_step_start_resets_flag() {
        let exec = StepProtocolExecutor::kilo("kilo".into());
        exec.parse_output_line(r#"{"type":"step_finish","part":{"type":"step_finish"}}"#);
        assert!(exec.check_success(144));
        exec.parse_output_line(r#"{"type":"step_start"}"#);
        assert!(!exec.check_success(144), "step_start 应重置成功标记");
    }

    /// NTD-012 钩子：on_pipeline_event 与 handler 语义逐字对齐
    #[test]
    fn test_on_pipeline_event_syncs_flag() {
        use crate::execution_events::ExecutionEvent as E;
        let exec = StepProtocolExecutor::mimo("mimo".into());
        exec.on_pipeline_event(&E::StepFinish { name: "s".into(), index: 0 });
        assert!(exec.check_success(144));
        exec.on_pipeline_event(&E::StepStart { name: "s".into(), index: 1 });
        assert!(!exec.check_success(144));
    }

    /// check_success 零退出码恒成功（与 flag 无关）
    #[test]
    fn test_check_success_exit_code_zero() {
        let exec = StepProtocolExecutor::kilo("kilo".into());
        assert!(exec.check_success(0));
    }

    /// session_id：顶层字段提取并缓存；无 sid 的行回退缓存值
    #[test]
    fn test_extract_session_id_cache_fallback() {
        let exec = StepProtocolExecutor::kilo("kilo".into());
        let sid = exec.extract_session_id(r#"{"type":"step-start","sessionID":"sess_abc"}"#);
        assert_eq!(sid, Some("sess_abc".to_string()));
        assert_eq!(
            exec.extract_session_id(r#"{"type":"text"}"#),
            Some("sess_abc".to_string()),
            "无 sid 字段的行应回退到缓存值"
        );
        assert_eq!(exec.get_session_id(), Some("sess_abc".to_string()));
    }

    /// session_id 初始为 None
    #[test]
    fn test_get_session_id_initial_none() {
        assert_eq!(StepProtocolExecutor::mimo("mimo".into()).get_session_id(), None);
    }

    /// get_model 差异：kilo 系回报注入模型；mimo 恒 None（差异矩阵第 5 格）
    #[test]
    fn test_get_model_differs_by_flavor() {
        let kilo = StepProtocolExecutor::kilo("kilo".into());
        kilo.set_exec_model(Some("provider/model".to_string()));
        assert_eq!(kilo.get_model(), Some("provider/model".to_string()));

        let mimo = StepProtocolExecutor::mimo("mimo".into());
        mimo.set_exec_model(Some("xiaomi/mimo-pro".to_string()));
        assert_eq!(mimo.get_model(), None, "mimo JSON 输出不含模型名，恒 None");
    }

    /// command_args 基础形态四家一致
    #[test]
    fn test_command_args_shape() {
        let exec = StepProtocolExecutor::kilo("kilo".into());
        let args = exec.command_args("hello");
        assert_eq!(args[0], "run");
        assert_eq!(args[1], "--format");
        assert_eq!(args[2], "json");
        assert_eq!(args[3], "--dangerously-skip-permissions");
        assert_eq!(args[4], "hello");
    }

    /// resume 差异：mimo 无 session_id 降级 -c；kilo 系不加参数（差异矩阵第 4 格）
    #[test]
    fn test_resume_without_session_differs_by_flavor() {
        let mimo_args = StepProtocolExecutor::mimo("mimo".into())
            .command_args_with_session("hello", None, true);
        assert!(mimo_args.contains(&"-c".to_string()), "mimo resume 无 sid 应降级 -c");

        let kilo_args = StepProtocolExecutor::kilo("kilo".into())
            .command_args_with_session("hello", None, true);
        assert!(!kilo_args.contains(&"-c".to_string()), "kilo 系 resume 无 sid 不加参数");
        assert!(!kilo_args.contains(&"-s".to_string()));
    }

    /// resume 带 session_id：四家都用 -s 精确恢复
    #[test]
    fn test_resume_with_session_uses_dash_s() {
        for exec in [
            StepProtocolExecutor::kilo("kilo".into()),
            StepProtocolExecutor::mimo("mimo".into()),
        ] {
            let args = exec.command_args_with_session("hello", Some("ses_1"), true);
            assert!(args.contains(&"-s".to_string()));
            assert!(args.contains(&"ses_1".to_string()));
        }
    }

    /// 模型注入：set_exec_model 后 command_args_with_session 拼 -m
    #[test]
    fn test_command_args_injects_model_when_set() {
        let exec = StepProtocolExecutor::mimo("mimo".into());
        let args_none = exec.command_args_with_session("hello", None, false);
        assert!(!args_none.iter().any(|a| a == "-m"));
        exec.set_exec_model(Some("xiaomi/mimo-v2.5-pro".to_string()));
        let args = exec.command_args_with_session("hello", None, false);
        let model_value = args.windows(2).find(|w| w[0] == "-m").map(|w| w[1].clone());
        assert_eq!(model_value, Some("xiaomi/mimo-v2.5-pro".to_string()));
    }

    /// executor_type 与注册名一一对应
    #[test]
    fn test_executor_type_mapping() {
        assert_eq!(StepProtocolExecutor::kilo("k".into()).executor_type(), ExecutorType::Kilo);
        assert_eq!(StepProtocolExecutor::opencode("o".into()).executor_type(), ExecutorType::Opencode);
        assert_eq!(StepProtocolExecutor::zhanlu("z".into()).executor_type(), ExecutorType::Zhanlu);
        assert_eq!(StepProtocolExecutor::mimo("m".into()).executor_type(), ExecutorType::Mimo);
    }

    /// get_final_result：text 去空白；空日志 None；result 优先于 text
    #[test]
    fn test_get_final_result_fallback_chain() {
        let exec = StepProtocolExecutor::kilo("kilo".into());
        let logs = vec![ParsedLogEntry::new("text", "  hello world  ")];
        assert_eq!(exec.get_final_result(&logs), Some("hello world".to_string()));
        let empty: Vec<ParsedLogEntry> = vec![];
        assert!(exec.get_final_result(&empty).is_none());
        let with_result = vec![
            ParsedLogEntry::new("text", "正文"),
            ParsedLogEntry::new("result", "结论"),
        ];
        assert_eq!(exec.get_final_result(&with_result), Some("结论".to_string()));
    }

    /// 真实协议 JSONL 形态端到端（kilo 采集样本）
    #[test]
    fn test_parse_real_world_jsonl_sequence() {
        let exec = StepProtocolExecutor::kilo("kilo".into());
        exec.parse_output_line(r#"{"type":"step-start","timestamp":1777471473403,"sessionID":"ses_xxx"}"#).unwrap();
        let text = exec
            .parse_output_line(r#"{"type":"text","timestamp":1777471505165,"sessionID":"ses_xxx","part":{"type":"text","text":"Hello"}}"#)
            .unwrap();
        assert_eq!(text.content, "Hello");
        exec.parse_output_line(r#"{"type":"step-finish","timestamp":1777471505168,"sessionID":"ses_xxx","part":{"type":"step-finish","reason":"stop","tokens":{"total":14155,"input":13862,"output":293,"reasoning":0,"cache":{"write":0,"read":0}},"cost":0}}"#).unwrap();
        assert!(exec.check_success(144), "完整事件序列后非零退出应判成功");
        assert_eq!(exec.get_session_id(), Some("ses_xxx".to_string()));
    }
}
