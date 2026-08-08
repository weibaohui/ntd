//! 工作空间设置的数据库访问层
//!
//! 提供 workspace_settings 表的 CRUD 操作。

use sea_orm::{ActiveModelTrait, ActiveValue, ColumnTrait, EntityTrait, IntoActiveModel, QueryFilter};
use crate::db::Database;

/// 获取工作空间设置
pub async fn get_workspace_settings(
    db: &Database,
    workspace_id: i64,
) -> Result<Option<crate::db::entity::workspace_settings::Model>, sea_orm::DbErr> {
    use crate::db::entity::workspace_settings as ws;

    let settings = ws::Entity::find()
        .filter(ws::Column::WorkspaceId.eq(workspace_id))
        .one(&db.conn)
        .await?;

    Ok(settings)
}

/// 创建或更新工作空间设置
///
/// 增量更新语义：传入 `None` 的字段保持原值不动；传入 `Some(v)` 的字段被覆写为 `v`。
/// 例外：`default_response_loop_id` 中 `Some(0)` 表示显式清空。
///
/// `system_prompt` 同样遵循增量语义：
/// - `Some(p)`（含空串）→ 覆写为 `p`，用户清空 prompt 时前端传 `Some("")`
/// - `None` → 不动该列，保留既有 prompt
pub async fn upsert_workspace_settings(
    db: &Database,
    workspace_id: i64,
    default_response_type: Option<String>,
    default_response_todo_id: Option<i64>,
    default_response_loop_id: Option<i64>,
    default_response_executor: Option<String>,
    system_prompt: Option<String>,
) -> Result<(), sea_orm::DbErr> {
    use crate::db::entity::workspace_settings as ws;

    let existing = ws::Entity::find()
        .filter(ws::Column::WorkspaceId.eq(workspace_id))
        .one(&db.conn)
        .await?;

    if let Some(model) = existing {
        // 更新：每个字段 Some 才覆写，None 跳过保留原值
        let mut am = model.into_active_model();
        if let Some(t) = default_response_type {
            am.default_response_type = ActiveValue::Set(t);
        }
        if let Some(todo_id) = default_response_todo_id {
            am.default_response_todo_id = ActiveValue::Set(Some(todo_id));
        }
        // loop_id = 0 表示清空
        if let Some(loop_id) = default_response_loop_id {
            if loop_id == 0 {
                am.default_response_loop_id = ActiveValue::Set(None);
            } else {
                am.default_response_loop_id = ActiveValue::Set(Some(loop_id));
            }
        }
        if let Some(exec) = default_response_executor {
            am.default_response_executor = ActiveValue::Set(Some(exec));
        }
        // system_prompt：Some(含空串) 覆写，None 不动
        if let Some(prompt) = system_prompt {
            am.system_prompt = ActiveValue::Set(Some(prompt));
        }
        am.updated_at = ActiveValue::Set(Some(crate::models::utc_timestamp()));
        am.update(&db.conn).await?;
    } else {
        // 创建：None 字段落 NULL，由调用方决定
        let now = crate::models::utc_timestamp();
        let am = ws::ActiveModel {
            // 新建记录时主键由 DB 自增，显式标记 NotSet
            id: ActiveValue::NotSet,
            workspace_id: ActiveValue::Set(workspace_id),
            default_response_type: ActiveValue::Set(default_response_type.unwrap_or_else(|| "todo".to_string())),
            default_response_todo_id: ActiveValue::Set(default_response_todo_id),
            default_response_loop_id: ActiveValue::Set(default_response_loop_id.filter(|&x| x != 0)),
            default_response_executor: ActiveValue::Set(default_response_executor),
            system_prompt: ActiveValue::Set(system_prompt),
            // upsert 不负责 relay-max（由专用 DAO update_workspace_delegate_max_rounds 管理），
            // 新建行时显式 NotSet → INSERT 落 NULL（=未配置，三级解析回退兜底常量）。
            delegate_max_rounds: ActiveValue::NotSet,
            updated_at: ActiveValue::Set(Some(now)),
        };
        am.insert(&db.conn).await?;
    }

    Ok(())
}

/// 单独更新工作空间的「委派接力轮数上限」默认（需求 092 护栏配置化）。
///
/// 刻意独立于 [`upsert_workspace_settings`]：后者是多字段位置式签名，11 处既有调用点多数与本字段
/// 无关（executor_service / feishu_listener 的响应配置流程），强加新参会迫使它们各补一个 `None`，
/// 既增加无关 diff、也放大冲突面。单字段更新走本函数，零波及既有调用，且「接力护栏」与「响应配置」
/// 本就是两个关注点，分而治之更清晰。
///
/// 语义（与任务级 [`crate::db::task::update_delegate_max_rounds`] 一致，便于记忆）：
/// - `Some(n)`（n≥1，由 handler 校验）→ 置为 n，任务级未覆盖时以此为准。
/// - `None` → 清空该列，回退终极兜底常量 `MAX_DELEGATE_ROUNDS`。
///
/// 行不存在时按 [`upsert_workspace_settings`] 同口径新建一行（`default_response_type` 默认 "todo"），
/// 仅本字段带值，其余走默认/NULL。
pub async fn update_workspace_delegate_max_rounds(
    db: &Database,
    workspace_id: i64,
    max: Option<i64>,
) -> Result<(), sea_orm::DbErr> {
    use crate::db::entity::workspace_settings as ws;

    let existing = ws::Entity::find()
        .filter(ws::Column::WorkspaceId.eq(workspace_id))
        .one(&db.conn)
        .await?;
    if let Some(model) = existing {
        // 既有行：仅改本列 + 刷新 updated_at，其余列不动。
        let mut am: ws::ActiveModel = model.into();
        am.delegate_max_rounds = ActiveValue::Set(max);
        am.updated_at = ActiveValue::Set(Some(crate::models::utc_timestamp()));
        am.update(&db.conn).await?;
    } else {
        // 无行：按 upsert 新建口径补一行，仅本字段带值。
        let now = crate::models::utc_timestamp();
        let am = ws::ActiveModel {
            id: ActiveValue::NotSet,
            workspace_id: ActiveValue::Set(workspace_id),
            default_response_type: ActiveValue::Set("todo".to_string()),
            default_response_todo_id: ActiveValue::Set(None),
            default_response_loop_id: ActiveValue::Set(None),
            default_response_executor: ActiveValue::Set(None),
            system_prompt: ActiveValue::Set(None),
            delegate_max_rounds: ActiveValue::Set(max),
            updated_at: ActiveValue::Set(Some(now)),
        };
        am.insert(&db.conn).await?;
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    /// 创建时传入 system_prompt，再读取能拿到相同值。
    #[tokio::test]
    async fn test_upsert_with_system_prompt() {
        let db = Database::new(":memory:").await.unwrap();
        let prompt = "## 工作空间共识\n- 产物目录：./target";
        upsert_workspace_settings(
            &db, 1, None, None, None, None, Some(prompt.to_string()),
        )
        .await
        .unwrap();
        let settings = get_workspace_settings(&db, 1).await.unwrap().unwrap();
        assert_eq!(settings.system_prompt.as_deref(), Some(prompt));
    }

    /// 已存在 system_prompt，再次 upsert 传 None 时旧值保持不变。
    #[tokio::test]
    async fn test_upsert_none_system_prompt_keeps_old() {
        let db = Database::new(":memory:").await.unwrap();
        let prompt = "原有共识";
        // 第一次写入 prompt
        upsert_workspace_settings(
            &db, 1, None, None, None, None, Some(prompt.to_string()),
        )
        .await
        .unwrap();
        // 第二次更新其他字段，system_prompt 传 None
        upsert_workspace_settings(
            &db, 1, Some("loop".to_string()), None, None, None, None,
        )
        .await
        .unwrap();
        let settings = get_workspace_settings(&db, 1).await.unwrap().unwrap();
        assert_eq!(settings.system_prompt.as_deref(), Some(prompt));
        assert_eq!(settings.default_response_type, "loop");
    }

    /// 显式传空串 Some("") 覆写原 prompt。
    #[tokio::test]
    async fn test_upsert_empty_string_clears_prompt() {
        let db = Database::new(":memory:").await.unwrap();
        // 先写入非空 prompt
        upsert_workspace_settings(
            &db, 1, None, None, None, None, Some("共识".to_string()),
        )
        .await
        .unwrap();
        // 显式传空串清空
        upsert_workspace_settings(
            &db, 1, None, None, None, None, Some(String::new()),
        )
        .await
        .unwrap();
        let settings = get_workspace_settings(&db, 1).await.unwrap().unwrap();
        assert_eq!(settings.system_prompt.as_deref(), Some(""));
    }

    /// update_workspace_delegate_max_rounds：无既有行时新建一行并写入上限。
    #[tokio::test]
    async fn test_update_workspace_delegate_max_rounds_inserts_when_absent() {
        let db = Database::new(":memory:").await.unwrap();
        update_workspace_delegate_max_rounds(&db, 7, Some(15))
            .await
            .unwrap();
        let s = get_workspace_settings(&db, 7).await.unwrap().unwrap();
        assert_eq!(s.delegate_max_rounds, Some(15));
        // 新建行走默认响应类型口径，避免空行误用。
        assert_eq!(s.default_response_type, "todo");
    }

    /// 既有行：置值后再传 None 清空（回退兜底），不影响其他列。
    #[tokio::test]
    async fn test_update_workspace_delegate_max_rounds_set_then_clear() {
        let db = Database::new(":memory:").await.unwrap();
        // 先建行并写入 system_prompt，验证单字段更新不误伤其他列。
        upsert_workspace_settings(&db, 1, None, None, None, None, Some("共识".to_string()))
            .await
            .unwrap();
        update_workspace_delegate_max_rounds(&db, 1, Some(20))
            .await
            .unwrap();
        let s = get_workspace_settings(&db, 1).await.unwrap().unwrap();
        assert_eq!(s.delegate_max_rounds, Some(20));
        assert_eq!(s.system_prompt.as_deref(), Some("共识"));
        // None = 清空回退兜底。
        update_workspace_delegate_max_rounds(&db, 1, None)
            .await
            .unwrap();
        let s2 = get_workspace_settings(&db, 1).await.unwrap().unwrap();
        assert_eq!(s2.delegate_max_rounds, None);
        // 其他列不受清空影响。
        assert_eq!(s2.system_prompt.as_deref(), Some("共识"));
    }
}
