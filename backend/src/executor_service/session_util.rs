//! 执行器会话（session）辅助函数。
//!
//! 本模块是 096-W1-PR3「跨文件重复收口」的落点：
//! `services::message_debounce`（飞书私聊通路）与 `services::blackboard`（wiki chat 通路）
//! 曾各自维护一份逐字相同的 `extract_session_from_logs` 私有函数（连 doc 注释都一致），
//! session 提取策略调整要改两处。这里上移到执行器服务层作为唯一公共实现，
//! 两个调用方仅保留各自的「成功后持久化到 DB」业务逻辑。

use std::sync::Arc;

use crate::adapters::CodeExecutor;
use crate::models::ParsedLogEntry;

/// 从执行日志中提取 session_id。
///
/// 流程：
/// 1. 先尝试从日志内容中提取（extract_session_id）
/// 2. 如果没有，尝试执行器内部缓存的 session_id（get_session_id）
///
/// 不同执行器暴露 session_id 的方式不同：
/// - Claude Code: stdout JSONL 行含 session_id
/// - Hermès: `session_id: <sid>` 行
/// - Pi: `{"type":"session","id":"<sid>"}` 行（通过 get_session_id 获取缓存值）
///
/// 返回 None 表示执行器不支持 session 或首次执行。
pub(crate) fn extract_session_from_logs(
    executor: &Arc<dyn CodeExecutor>,
    logs: &[ParsedLogEntry],
) -> Option<String> {
    // 1. 优先从日志内容提取：逐行询问执行器，首个命中即返回——
    //    同一 session 的 id 在多行重复出现时取首个，避免尾部临时行覆盖真实会话 id。
    for entry in logs {
        if let Some(sid) = executor.extract_session_id(&entry.content) {
            return Some(sid);
        }
    }
    // 2. 回退到执行器内部缓存的 session_id（Pi 等执行器在 parse_output_line 时缓存）
    executor.get_session_id()
}

#[cfg(test)]
// 测试夹具允许 unwrap/expect：与 pre_spawn 等既有测试模块的 lint 豁免惯例保持一致
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    // ExecutorType 定义在 models 层（adapters 内只是私有 use，不可跨模块引用）
    use crate::models::ExecutorType;
    use async_trait::async_trait;

    /// 可控的最小执行器桩：按「行内容是否含 marker」模拟 extract_session_id 的命中/未命中，
    /// cached_sid 模拟执行器内部缓存（Pi 模式）。adapters 测试模块的 MockExecutor 是模块私有的，
    /// 跨模块测试不可引用，故在此自备桩实现。
    struct StubExecutor {
        /// extract_session_id 只在行内容含该 marker 时返回 sid
        marker: &'static str,
        /// 命中时返回的 session_id
        line_sid: Option<String>,
        /// get_session_id 的固定返回（模拟内部缓存）
        cached_sid: Option<String>,
    }

    #[async_trait]
    impl CodeExecutor for StubExecutor {
        fn executor_type(&self) -> ExecutorType {
            ExecutorType::Mobilecoder
        }
        fn executable_path(&self) -> &str {
            "stub"
        }
        fn command_args(&self, _message: &str) -> Vec<String> {
            vec![]
        }
        fn parse_output_line(&self, _line: &str) -> Option<ParsedLogEntry> {
            None
        }
        fn get_model(&self) -> Option<String> {
            None
        }
        fn extract_session_id(&self, line: &str) -> Option<String> {
            // 仅当行内容包含 marker 时视为命中，模拟真实执行器的行级解析
            if line.contains(self.marker) {
                self.line_sid.clone()
            } else {
                None
            }
        }
        fn get_session_id(&self) -> Option<String> {
            self.cached_sid.clone()
        }
    }

    /// 日志行中可提取时应返回首个命中的 session_id（Claude Code / Hermès 模式）。
    #[test]
    fn test_extract_session_from_logs_hit_in_logs_returns_first_match() {
        let executor: Arc<dyn CodeExecutor> = Arc::new(StubExecutor {
            marker: "session_id",
            line_sid: Some("sid-from-line".to_string()),
            cached_sid: Some("sid-cached".to_string()),
        });
        let logs = vec![
            ParsedLogEntry::new("info", "启动执行"),
            ParsedLogEntry::new("info", "session_id: sid-from-line"),
        ];
        // 行内命中优先于缓存：即使缓存有值，也以日志解析结果为准
        assert_eq!(
            extract_session_from_logs(&executor, &logs),
            Some("sid-from-line".to_string())
        );
    }

    /// 日志行全部未命中时回退到执行器内部缓存（Pi 模式）。
    #[test]
    fn test_extract_session_from_logs_fallback_to_cached() {
        let executor: Arc<dyn CodeExecutor> = Arc::new(StubExecutor {
            marker: "session_id",
            line_sid: Some("sid-from-line".to_string()),
            cached_sid: Some("sid-cached".to_string()),
        });
        let logs = vec![ParsedLogEntry::new("text", "普通输出，没有会话信息")];
        assert_eq!(
            extract_session_from_logs(&executor, &logs),
            Some("sid-cached".to_string())
        );
    }

    /// 日志未命中且缓存为空时返回 None（执行器不支持 session 或首次执行）。
    #[test]
    fn test_extract_session_from_logs_no_hit_no_cache_returns_none() {
        let executor: Arc<dyn CodeExecutor> = Arc::new(StubExecutor {
            marker: "session_id",
            line_sid: None,
            cached_sid: None,
        });
        let logs = vec![ParsedLogEntry::new("text", "普通输出")];
        assert_eq!(extract_session_from_logs(&executor, &logs), None);
        // 空日志列表同样走缓存路径，结果也为 None
        assert_eq!(extract_session_from_logs(&executor, &[]), None);
    }
}
