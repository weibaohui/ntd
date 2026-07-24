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
}
