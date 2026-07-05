//! 自动评审（auto-review）—— 工具函数层
//!
//! 本模块的职责（V15 + review_templates 重构后）：
//! 1. 默认评审 prompt 模板定义（`DEFAULT_REVIEWER_PROMPT`）
//! 2. 评审结果 RATING 解析（`parse_rating_from_result`）
//! 3. 评审 prompt 中"截断原 todo 输出"的最大字符数（`MAX_OUTPUT_CHARS`）
//! 4. 同步启动钩子：确保默认评审模板存在（`ensure_default_review_template`，
//!    委托给 `db::review_template::ensure_default_review_template`）
//!
//! 数据流：
//! 1. executor_service 检测到原 todo (todo_type=0) 完成
//! 2. 调 `db::ensure_default_review_template` 取得 review_template.id
//! 3. 截断原 record.output，合并 prompt，clone 出"评审实例" todo (todo_type=2)
//! 4. 同步调 run_todo_execution 跑评审实例
//! 5. 轮询评审实例 record 进入终态
//! 6. 解析 RATING 回填到原 execution_record.rating
//! 7. last_review_status 写终态
//!
//! 历史：本模块以前还负责"评审任务" todo (todo_type=1) 的初始化与旧标题重命名。
//! V15 迁移把 todo_type=1 行搬到独立 `review_templates` 表后，这部分职责迁出，
//! 见 `db::review_template::ensure_default_review_template`。

use std::sync::Arc;
use tracing::{info, warn};

/// 评审 prompt 中"截断原 todo 输出"的最大字符数（Q9 决定）。
pub const MAX_OUTPUT_CHARS: usize = 8_000;

/// 评审 prompt 模板 —— 启动时如果 review_templates 表为空，会用这段作为初始 prompt。
/// 用户可随时编辑模板自定义评审风格；这段只是默认值。
pub const DEFAULT_REVIEWER_PROMPT: &str = r#"你是一个严格的代码评审专家。请根据下方【验收标准】对【执行输出】进行评审。

评审范围：
- 只读不写 —— 你是裁判，不修改任何文件、不执行额外命令。
- 按验收标准逐条对照执行输出。
- 客观评分，不要讨好。

# 原始任务（用户给原 todo 的 prompt）
{original_prompt}

# 原 todo 的执行输出（已被截断到 {max_output_chars} 字符）
{original_output}

# 验收标准
{acceptance_criteria}

# 输出要求
请先给出一段简短的评审理由（不超过 200 字），然后**在最后一行**严格按以下格式输出分数：
RATING: <0-100 的整数>

注意：
- 分数必须是 0-100 的整数。
- 输出越严格、越符合所有验收标准，分数越高。
- 如果输出被截断且关键证据缺失，请在理由中说明"输出被截断"并按缺失程度扣分。
"#;

/// 同步确保默认评审模板存在（review_templates 表里 name="默认评审任务"）。
/// 由启动期 `create_app` 调用一次（不在 reactor 线程 .await），失败仅 warn。
///
/// 设计原因：DAO 的同名方法是 async（`sea_orm::DbErr` 返回类型），
/// 而 `ensure_reviewer_template_blocking` 在启动期同步上下文里跑，
/// 需要包一层把错误转为 `String`。
pub fn ensure_default_review_template(db: &Arc<crate::db::Database>) -> Result<i64, String> {
    // 通过 tokio Handle 同步跑一次异步 DAO。
    // 与 `ensure_reviewer_template_blocking` 同样要求处于 tokio runtime 内。
    tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async {
            db.ensure_default_review_template().await.map_err(|e| {
                format!("ensure default review template: {}", e)
            })
        })
    })
}

/// 从评审师输出中解析 RATING: N
/// 接受 "RATING: 85" / "rating: 85" / "RATING = 85" / "最终评分：78" 等变体。
/// 必须落在 0..=100 范围，否则丢弃。
pub fn parse_rating_from_result(result: Option<&str>) -> Option<i32> {
    let text = result?;
    // 取最后 50 行（评分通常在末尾）
    let tail: String = text
        .lines()
        .rev()
        .take(50)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("\n");
    for line in tail.lines().rev() {
        let l = line.trim();
        let lower = l.to_lowercase();
        if !(lower.contains("rating") || lower.contains("评分")) {
            continue;
        }
        // 形如 "RATING: 85" / "Rating: 85" / "RATING = 85" / "最终评分: 85"
        let after = l
            .split_once(':')
            .or_else(|| l.split_once('：'))
            .or_else(|| l.split_once('='))
            .map(|(_, v)| v.trim());
        if let Some(v) = after {
            // 抓第一个整数
            let digits: String = v
                .chars()
                .skip_while(|c| !c.is_ascii_digit() && *c != '-')
                .take_while(|c| c.is_ascii_digit() || *c == '-')
                .collect();
            if let Ok(n) = digits.parse::<i32>() {
                if (0..=100).contains(&n) {
                    return Some(n);
                }
            }
        }
    }
    None
}

/// 在非 tokio 上下文中调用 ensure_default_review_template 的便捷包装。
/// 失败仅 warn 不 panic（启动期初始化失败不应阻塞 daemon）。
pub fn ensure_default_review_template_blocking(db: &Arc<crate::db::Database>) {
    match ensure_default_review_template(db) {
        Ok(id) => info!("default review template ready (id={})", id),
        Err(e) => warn!("failed to ensure default review template: {}", e),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
mod tests {
    use super::*;

    #[test]
    fn parse_rating_basic() {
        assert_eq!(
            parse_rating_from_result(Some("Some reasoning.\nRATING: 85")),
            Some(85)
        );
    }

    #[test]
    fn parse_rating_chinese_colon() {
        assert_eq!(
            parse_rating_from_result(Some("评审通过。\n最终评分： 78")),
            Some(78)
        );
    }

    #[test]
    fn parse_rating_equals() {
        assert_eq!(parse_rating_from_result(Some("ok\nrating = 90")), Some(90));
    }

    #[test]
    fn parse_rating_out_of_range() {
        assert_eq!(parse_rating_from_result(Some("RATING: 150")), None);
        assert_eq!(parse_rating_from_result(Some("RATING: -5")), None);
    }

    #[test]
    fn parse_rating_missing() {
        assert_eq!(parse_rating_from_result(Some("no score here")), None);
        assert_eq!(parse_rating_from_result(None), None);
    }

    #[test]
    fn parse_rating_takes_last_line() {
        // 即使中间有干扰，最后的 RATING 应该赢
        let text = "RATING: 30\nmore text\nRATING: 88";
        assert_eq!(parse_rating_from_result(Some(text)), Some(88));
    }

    // ── DEFAULT_REVIEWER_PROMPT 快照测试 ──
    // 重构后仍保留这个常量供 DAO 与启动路径使用，
    // 防止有人手抖把内容删成空字符串或换行符。
    #[test]
    fn default_reviewer_prompt_contains_required_placeholders() {
        assert!(DEFAULT_REVIEWER_PROMPT.contains("{original_prompt}"));
        assert!(DEFAULT_REVIEWER_PROMPT.contains("{original_output}"));
        assert!(DEFAULT_REVIEWER_PROMPT.contains("{acceptance_criteria}"));
        assert!(DEFAULT_REVIEWER_PROMPT.contains("RATING"));
        assert!(DEFAULT_REVIEWER_PROMPT.contains("评审"));
    }
}