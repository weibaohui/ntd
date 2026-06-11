//! 自动评审（auto-review）—— 工具函数层
//!
//! 本模块的职责：
//! 1. 评审师模板 todo 的初始化（ensure_reviewer_template）
//! 2. 评审 prompt 模板定义（DEFAULT_REVIEWER_PROMPT）
//! 3. 评审结果 RATING 解析（parse_rating_from_result）
//!
//! 实际"同步跑评审实例"的协调逻辑在 `executor_service::run_auto_review`，
//! 它直接调用 ensure_reviewer_template / parse_rating_from_result，避免循环依赖。
//!
//! 数据流：
//! 1. executor_service 检测到原 todo (todo_type=0) 完成
//! 2. 调 ensure_reviewer_template 取得"评审师模板" todo id
//! 3. 截断原 record.output，合并 prompt，clone 出"评审实例" todo
//! 4. 同步调 run_todo_execution 跑评审实例
//! 5. 轮询评审实例 record 进入终态
//! 6. 解析 RATING 回填到原 execution_record.rating
//! 7. last_review_status 写终态

use std::sync::Arc;
use tracing::info;

/// 评审师模板 todo 的固定标题（同时作为唯一标识）。
pub const REVIEWER_TEMPLATE_TITLE: &str = "评审师模板";

/// 评审 prompt 中"截断原 todo 输出"的最大字符数（Q9 决定）。
pub const MAX_OUTPUT_CHARS: usize = 8_000;

/// 评审 prompt 模板 —— 启动时如果没有"评审师模板" todo，会用这段作为初始 prompt。
/// 用户可随时编辑模板 todo 自定义评审风格；这段只是默认值。
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

/// 确保数据库中存在"评审师模板" todo (todo_type=1).
/// 如果用户已经有一个（按 title 查找），什么都不做（保留用户改过的 prompt）。
/// 如果没有，创建一个 todo_type=1 的系统专用 todo。
pub async fn ensure_reviewer_template(
    db: &Arc<crate::db::Database>,
    title: &str,
    default_prompt: &str,
) -> Result<i64, String> {
    match db.get_todo_by_title(title).await {
        Ok(Some(t)) => {
            if t.todo_type != 1 {
                db.set_todo_type(t.id, 1).await
                    .map_err(|e| format!("set reviewer template todo_type: {}", e))?;
            }
            Ok(t.id)
        }
        Ok(None) => {
            let id = db
                .create_todo_with_extras(title, default_prompt, None, None)
                .await
                .map_err(|e| format!("create reviewer template: {}", e))?;
            db.set_todo_type(id, 1)
                .await
                .map_err(|e| format!("set new reviewer template type: {}", e))?;
            // 模板 todo 不应触发自动评审自身
            let _ = db.update_todo_auto_review_enabled(id, false).await;
            info!("created auto-review reviewer template todo #{}", id);
            Ok(id)
        }
        Err(e) => Err(format!("lookup reviewer template: {}", e)),
    }
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

#[cfg(test)]
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
}
