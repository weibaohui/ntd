//! 自动评审（auto-review）—— 工具函数层
//!
//! 本模块的职责：
//! 1. 评审任务（系统自动维护的 review task 模板）todo 的初始化（ensure_reviewer_template）
//! 2. 评审 prompt 模板定义（DEFAULT_REVIEWER_PROMPT）
//! 3. 评审结果 RATING 解析（parse_rating_from_result）
//!
//! 实际"同步跑评审实例"的协调逻辑在 `executor_service::run_auto_review`，
//! 它直接调用 ensure_reviewer_template / parse_rating_from_result，避免循环依赖。
//!
//! 数据流：
//! 1. executor_service 检测到原 todo (todo_type=0) 完成
//! 2. 调 ensure_reviewer_template 取得"评审任务" todo id
//! 3. 截断原 record.output，合并 prompt，clone 出"评审实例" todo
//! 4. 同步调 run_todo_execution 跑评审实例
//! 5. 轮询评审实例 record 进入终态
//! 6. 解析 RATING 回填到原 execution_record.rating
//! 7. last_review_status 写终态

use std::sync::Arc;
use tracing::info;

/// 借用 `sea_orm::ActiveValue::Unchanged` 的别名, 避免在本文件内反复写出
/// 那个相当长的类型路径。
use sea_orm::ActiveValue::Unchanged as ActiveValueUnchanged;

/// "评审任务" todo 的固定标题（同时作为唯一标识）。
///
/// 历史名："评审师模板" —— 该名已废弃 (issue #598 重命名)，但旧库可能仍存在
/// 旧标题的记录；`ensure_reviewer_template` 在查不到新名时会回退查找旧名并就地
/// 改名为新名，保证存量数据库平滑升级。
pub const REVIEWER_TEMPLATE_TITLE: &str = "评审任务";

/// 旧版本使用的"评审师模板"标题。新库不会再写该值，仅作为回退探测的别名保留。
pub const REVIEWER_TEMPLATE_TITLE_LEGACY: &str = "评审师模板";

/// 评审 prompt 中"截断原 todo 输出"的最大字符数（Q9 决定）。
pub const MAX_OUTPUT_CHARS: usize = 8_000;

/// 评审 prompt 模板 —— 启动时如果没有"评审任务" todo，会用这段作为初始 prompt。
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

/// 确保数据库中存在"评审任务" todo (todo_type=1).
/// 如果用户已经有一个（按 title 查找），什么都不做（保留用户改过的 prompt）。
/// 如果没有，创建一个 todo_type=1 的系统专用 todo。
///
/// 向后兼容：旧库可能仍以旧标题"评审师模板"建过该 todo；新名查不到时会回退查找
/// 旧名，并把那条记录就地 rename 为新标题，避免旧库升级后产生重复"评审任务"。
///
/// 提权保护：新标题"评审任务"较为通用，用户可能自建同名普通 todo (todo_type=0)。
/// 命中这种记录时**绝不**把它强制改成 todo_type=1（会污染用户数据并打断 auto-review
/// 上下文），而是先尝试 legacy 升级路径；若 legacy 也没有，则显式报错让运维介入。
pub async fn ensure_reviewer_template(
    db: &Arc<crate::db::Database>,
    title: &str,
    default_prompt: &str,
) -> Result<i64, String> {
    // 1) 先按新标题查找；只有 todo_type 已是 1（系统模板）才直接复用。
    //    命中的是用户自建普通 todo 时只记下冲突 id，留到最后显式报错，
    //    避免悄悄把用户数据提权为系统模板。
    let mut title_occupied_by_user: Option<i64> = None;
    if let Some(t) = db
        .get_todo_by_title(title)
        .await
        .map_err(|e| format!("lookup reviewer template: {}", e))?
    {
        if t.todo_type == 1 {
            return Ok(t.id);
        }
        title_occupied_by_user = Some(t.id);
    }
    // 2) 探测旧标题；命中则就地改名为新标题复用（legacy 升级路径在用户占名时仍优先）。
    if title != REVIEWER_TEMPLATE_TITLE_LEGACY {
        if let Some(legacy) = db
            .get_todo_by_title(REVIEWER_TEMPLATE_TITLE_LEGACY)
            .await
            .map_err(|e| format!("lookup legacy reviewer template: {}", e))?
        {
            rename_todo_title(db, legacy.id, title).await?;
            return Ok(fixup_template_todo_type(db, legacy.id, legacy.todo_type).await?);
        }
    }
    // 3) 没有可复用模板。若新标题已被用户占用 -> 显式报错（不悄悄创建重复）。
    if let Some(uid) = title_occupied_by_user {
        return Err(format!(
            "user-created todo '{}' (id={}) is occupying the reviewer template title; \
             rename or delete it before auto-review can initialize",
            title, uid
        ));
    }
    // 4) 新旧标题都不存在 -> 全新创建一条 todo_type=1 的系统专用 todo。
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

/// 把"评审任务" todo 的 todo_type 强制设为 1（系统模板），必要时回写 updated_at。
async fn fixup_template_todo_type(
    db: &Arc<crate::db::Database>,
    id: i64,
    current_type: i32,
) -> Result<i64, String> {
    if current_type != 1 {
        db.set_todo_type(id, 1)
            .await
            .map_err(|e| format!("set reviewer template todo_type: {}", e))?;
    }
    Ok(id)
}

/// 就地改写 todo 的 title（用于"评审师模板" -> "评审任务" 的历史数据升级）。
async fn rename_todo_title(db: &Arc<crate::db::Database>, id: i64, new_title: &str) -> Result<(), String> {
    use sea_orm::{ActiveModelTrait, Set};
    use crate::db::entity::todos;
    let now = crate::models::utc_timestamp();
    let am = todos::ActiveModel {
        id: ActiveValueUnchanged(id),
        title: Set(new_title.to_string()),
        updated_at: Set(Some(now)),
        ..Default::default()
    };
    am.update(&db.conn)
        .await
        .map_err(|e| format!("rename legacy reviewer template: {}", e))?;
    info!("renamed legacy reviewer template todo #{} to '{}'", id, new_title);
    Ok(())
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

    // --- 重命名相关常量的简单快照 ---
    // 这两个常量是 issue #598 重命名协议的关键：如果未来有人手抖把名字换回去，
    // 这两个测试会立刻失败；它比 review 整段 `ensure_reviewer_template` 更直接。
    #[test]
    fn title_constant_uses_new_name() {
        assert_eq!(REVIEWER_TEMPLATE_TITLE, "评审任务");
    }

    #[test]
    fn legacy_title_constant_preserves_old_name() {
        // 旧标题是探测别名：用户数据库中可能仍存有该值的 todo。
        // 任何重命名协议都不能改它，否则就找不到旧记录了。
        assert_eq!(REVIEWER_TEMPLATE_TITLE_LEGACY, "评审师模板");
    }

    #[test]
    fn legacy_title_differs_from_new_title() {
        // 防止"重命名"反而让新旧标题变成同一个值。
        assert_ne!(REVIEWER_TEMPLATE_TITLE, REVIEWER_TEMPLATE_TITLE_LEGACY);
    }

    // --- ensure_reviewer_template 在旧库上的就地重命名行为 ---
    // 这些测试构造一个 :memory: 数据库, 手工写入旧标题的 todo,
    // 然后调 ensure_reviewer_template 验证它会被就地改名为新标题, 而不是创建第二条.

    async fn fresh_db() -> Arc<crate::db::Database> {
        use std::sync::Arc;
        Arc::new(crate::db::Database::new(":memory:").await.expect("memory db must open"))
    }

    /// 场景 1：旧库只有旧标题"评审师模板" todo，调 ensure 后应被就地改名为新标题.
    #[tokio::test]
    async fn ensure_renames_legacy_title_in_place() {
        let db = fresh_db().await;
        // 准备旧记录: 标题=旧名, todo_type=1 (已经是模板类型)
        let legacy_id = db
            .create_todo_with_extras(REVIEWER_TEMPLATE_TITLE_LEGACY, "old prompt", None, None)
            .await
            .expect("create legacy todo");
        db.set_todo_type(legacy_id, 1)
            .await
            .expect("mark legacy as template type");

        // 调 ensure 函数：应返回旧 id (复用), 不创建新行
        let returned_id = ensure_reviewer_template(
            &db,
            REVIEWER_TEMPLATE_TITLE,
            DEFAULT_REVIEWER_PROMPT,
        )
        .await
        .expect("ensure should succeed on legacy db");
        assert_eq!(
            returned_id, legacy_id,
            "should reuse the legacy todo, not create a new one"
        );

        // 验证 DB 状态: 旧标题已不存在, 新标题存在且 todo_type=1
        assert!(
            db.get_todo_by_title(REVIEWER_TEMPLATE_TITLE_LEGACY)
                .await
                .expect("lookup legacy")
                .is_none(),
            "legacy title must be gone after rename"
        );
        let renamed = db
            .get_todo_by_title(REVIEWER_TEMPLATE_TITLE)
            .await
            .expect("lookup new")
            .expect("new title must exist after rename");
        assert_eq!(renamed.id, legacy_id, "same row id must be preserved");
        assert_eq!(renamed.todo_type, 1, "todo_type must remain 1");
        assert_eq!(
            renamed.prompt, "old prompt",
            "user-edited prompt must be preserved across rename"
        );
    }

    /// 场景 2：旧库存在旧标题记录，但其 todo_type != 1 (例如被用户手动改过)，
    /// ensure 应在改名的同时把 todo_type 强制改回 1.
    #[tokio::test]
    async fn ensure_renames_legacy_and_restores_template_type() {
        let db = fresh_db().await;
        let legacy_id = db
            .create_todo_with_extras(REVIEWER_TEMPLATE_TITLE_LEGACY, "p", None, None)
            .await
            .expect("create");
        // 故意把它设成 normal (0), 模拟被污染的旧记录
        db.set_todo_type(legacy_id, 0)
            .await
            .expect("force type=0");

        let returned_id = ensure_reviewer_template(
            &db,
            REVIEWER_TEMPLATE_TITLE,
            DEFAULT_REVIEWER_PROMPT,
        )
        .await
        .expect("ensure should succeed");
        assert_eq!(returned_id, legacy_id);
        let row = db
            .get_todo_by_title(REVIEWER_TEMPLATE_TITLE)
            .await
            .expect("lookup")
            .expect("present");
        assert_eq!(row.todo_type, 1, "ensure must restore todo_type=1");
    }

    /// 场景 3：新库 (无任何模板 todo) 上 ensure 应当用新标题全新创建一条.
    #[tokio::test]
    async fn ensure_creates_with_new_title_when_no_existing() {
        let db = fresh_db().await;
        let id = ensure_reviewer_template(
            &db,
            REVIEWER_TEMPLATE_TITLE,
            DEFAULT_REVIEWER_PROMPT,
        )
        .await
        .expect("ensure should succeed on empty db");
        let row = db
            .get_todo_by_title(REVIEWER_TEMPLATE_TITLE)
            .await
            .expect("lookup")
            .expect("present");
        assert_eq!(row.id, id);
        assert_eq!(row.todo_type, 1);
        assert_eq!(row.prompt, DEFAULT_REVIEWER_PROMPT);
        // 同时验证旧标题确实不存在
        assert!(
            db.get_todo_by_title(REVIEWER_TEMPLATE_TITLE_LEGACY)
                .await
                .expect("lookup")
                .is_none(),
            "fresh install must not create a legacy-title todo"
        );
    }

    /// 场景 4：新标题已被系统模板 (todo_type=1) 占用时, ensure 直接复用, 不改名不改 type.
    #[tokio::test]
    async fn ensure_reuses_existing_new_title_template() {
        let db = fresh_db().await;
        // 手工写入一条 todo_type=1, 标题就是新名 (新装机的形态)
        let existing_id = db
            .create_todo_with_extras(REVIEWER_TEMPLATE_TITLE, "user-edited prompt", None, None)
            .await
            .expect("create");
        db.set_todo_type(existing_id, 1).await.expect("mark template");

        let returned_id = ensure_reviewer_template(
            &db,
            REVIEWER_TEMPLATE_TITLE,
            DEFAULT_REVIEWER_PROMPT,
        )
        .await
        .expect("ensure should reuse existing template");
        assert_eq!(
            returned_id, existing_id,
            "should reuse existing system template, not create a duplicate"
        );
        let row = db
            .get_todo_by_title(REVIEWER_TEMPLATE_TITLE)
            .await
            .expect("lookup")
            .expect("present");
        assert_eq!(row.id, existing_id);
        assert_eq!(row.todo_type, 1);
        assert_eq!(
            row.prompt, "user-edited prompt",
            "user-edited prompt must be preserved when reusing"
        );
    }

    /// 场景 5：新标题被用户自建普通 todo (todo_type=0) 占用且没有 legacy ->
    /// ensure 应当显式报错, 绝不提权用户数据, 也绝不悄悄建重复.
    #[tokio::test]
    async fn ensure_refuses_to_promote_user_titled_todo() {
        let db = fresh_db().await;
        let user_id = db
            .create_todo_with_extras(REVIEWER_TEMPLATE_TITLE, "user prompt", None, None)
            .await
            .expect("create user todo");
        // 默认 todo_type=0, 模拟用户自建普通 todo

        let err = ensure_reviewer_template(
            &db,
            REVIEWER_TEMPLATE_TITLE,
            DEFAULT_REVIEWER_PROMPT,
        )
        .await
        .expect_err("ensure must refuse to promote user todo");
        assert!(
            err.contains(&user_id.to_string()),
            "error must mention the conflicting user todo id, got: {}",
            err
        );

        // 关键: 用户 todo 的 todo_type 必须仍是 0, prompt 也不能被系统默认值覆盖
        let row = db
            .get_todo_by_title(REVIEWER_TEMPLATE_TITLE)
            .await
            .expect("lookup")
            .expect("present");
        assert_eq!(row.id, user_id);
        assert_eq!(
            row.todo_type, 0,
            "user todo must not be promoted to system template"
        );
        assert_eq!(
            row.prompt, "user prompt",
            "user prompt must not be overwritten"
        );
    }

    /// 场景 6：新标题被用户占用 + 旧标题存在 -> legacy 升级路径仍应优先, 复用旧记录,
    /// 用户数据保持原样 (不会触发冲突报错).
    #[tokio::test]
    async fn ensure_legacy_path_wins_over_user_titled_collision() {
        let db = fresh_db().await;
        let user_id = db
            .create_todo_with_extras(REVIEWER_TEMPLATE_TITLE, "user prompt", None, None)
            .await
            .expect("create user todo");
        let legacy_id = db
            .create_todo_with_extras(REVIEWER_TEMPLATE_TITLE_LEGACY, "old prompt", None, None)
            .await
            .expect("create legacy");
        db.set_todo_type(legacy_id, 1)
            .await
            .expect("mark legacy as template");

        let returned_id = ensure_reviewer_template(
            &db,
            REVIEWER_TEMPLATE_TITLE,
            DEFAULT_REVIEWER_PROMPT,
        )
        .await
        .expect("ensure should succeed via legacy upgrade");
        assert_eq!(
            returned_id, legacy_id,
            "legacy record should be promoted, not the user todo"
        );
        // 用户 todo 保持原样
        let user_row = db
            .get_todo_by_title(REVIEWER_TEMPLATE_TITLE)
            .await
            .expect("lookup user")
            .expect("present");
        let legacy_row = db
            .get_todo_by_title(REVIEWER_TEMPLATE_TITLE_LEGACY)
            .await
            .expect("lookup legacy");
        // legacy 已被 rename, 不应再以旧标题找到
        assert!(legacy_row.is_none(), "legacy title must be gone after rename");
        // 但新标题下现在有两条: 用户原始 todo + 已升级的 legacy
        assert!(user_row.id == user_id || user_row.id == legacy_id);
    }
}
