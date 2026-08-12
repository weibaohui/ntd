//! Session 扫描器族（096-W3-PR2：从 `handlers/session.rs` 拆出的子模块）。
//!
//! ## 背景
//! session.rs 原 2890 行中仅 4 个真 handler，其余 ~55 个函数全是磁盘扫描 + JSONL 解析，
//! 而 `SCANNERS` 注册表的 6 个 impl 只是空壳转发（扫描报告 C5「多态抽象未竟」）。
//! 本模块把 6 个 scan_X / get_X_detail 函数体收编进各执行器文件，与其 Scanner 同址。
//!
//! ## 布局
//! - 本文件：跨执行器共享 helper（home_dir / truncate_str / extract_text_content /
//!   iter_jsonl_files / ParsedSessionLine），对子模块天然可见，无需 pub；
//! - `claude_code.rs` / `codex.rs` / `hermes.rs` / `kimi.rs` / `atomcode.rs` / `pi.rs`：
//!   各执行器的 Scanner struct + SessionScanner impl + 扫描/详情函数族 + 单测。
//!
//! 所有函数体为逐字搬迁（Move Function），未做任何逻辑改动。

use std::path::PathBuf;

pub mod atomcode;
pub mod claude_code;
pub mod codex;
pub mod hermes;
pub mod kimi;
pub mod pi;

// Scanner 类型 re-export：session.rs 的 SCANNERS 注册表经此引用
pub use atomcode::AtomCodeScanner;
pub use claude_code::ClaudeCodeScanner;
pub use codex::CodexScanner;
pub use hermes::HermesScanner;
pub use kimi::KimiScanner;
pub use pi::PiScanner;

/// Parsed metadata extracted from a single Claude Code JSONL line.
///
/// 用 9 字段位置元组会让调用点必须靠注释/心智模型对齐 `(ts, model, branch, ver, entry, content, inp, out, role)`，
/// 改字段顺序或新增字段极易破坏解构。改为具名字段后,
/// 调用点写 `meta.timestamp` / `meta.input_tokens` 自解释,改字段顺序零影响。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedSessionLine {
    /// 日志行时间戳;仅在原 JSON 含 `timestamp` 字段时存在
    pub timestamp: Option<String>,
    /// assistant 消息的 model 名;user/queue 行无 model
    pub model: Option<String>,
    /// Claude Code 会话关联的 git 分支(仅 user 行携带)
    pub git_branch: Option<String>,
    /// Claude Code 版本号(仅 user 行携带)
    pub version: Option<String>,
    /// 入口点(CLI / SDK / web 等,仅 user 行携带)
    pub entrypoint: Option<String>,
    /// 文本内容:user 取 `message.content`,queue 取顶层 `content`
    pub prompt: Option<String>,
    /// assistant message.usage.input_tokens
    pub input_tokens: Option<u64>,
    /// assistant message.usage.output_tokens
    pub output_tokens: Option<u64>,
    /// 归一化角色: `"user"` / `"assistant"` / `"queue"`
    pub role: String,
}

// 读取家目录的 helper。生产进程启动后 `dirs::home_dir()` 在极端环境
// (chroot/SELinux 拒绝) 才可能返回 None；按 codebase 约定回退到 /tmp，
// 与 `npm_utils.rs::get_npm_global_root` 等其它调用点保持一致。这个
// helper 有 18 个调用方跨 6 个 session scanner,一处 panic 会 cascade
// 到全部 6 个 —— 用 unwrap_or_else 而不是 expect 把 panic 风险归零。
pub(super) fn home_dir() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"))
}

/// Truncate a string to at most `max_len` chars, appending "..." if truncated.
fn truncate_str(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_len).collect();
        format!("{}...", truncated)
    }
}

/// Extract text content from a JSON value that may be a string or array of content blocks.
fn extract_text_content(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(blocks) => {
            let mut texts = Vec::new();
            for block in blocks {
                if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                    texts.push(text.to_string());
                }
            }
            texts.join("\n")
        }
        _ => String::new(),
    }
}

/// 在子目录中枚举 `*.jsonl` 文件,以 `(path, file_name)` 形式产出。
///
/// 抽出这个 helper 是因为 claude-code / hermes / pi 三个 scanner 都按
/// "目录下的 *.jsonl" 模式枚举,各自内联会让代码重复约 3 段同款 `if
/// path.extension() == Some("jsonl")` 守卫。kimi 的 `context.jsonl` 是
/// 固定名,codex 的 `rollout-*.jsonl` 有额外前缀——这两个仍保留各自内联过滤。
/// pub(super)：session.rs 保留的边界测试直接调用本函数。
pub(super) fn iter_jsonl_files(dir: &std::path::Path) -> Vec<(std::path::PathBuf, String)> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
            let name = entry.file_name().to_string_lossy().to_string();
            out.push((path, name));
        }
    }
    out
}
