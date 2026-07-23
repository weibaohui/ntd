//! 快捷话术按钮的数据库访问层
//!
//! 提供 quick_buttons 表的 CRUD 操作。按 workspace 隔离。

use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, EntityTrait, IntoActiveModel, QueryFilter,
    QueryOrder,
};
use crate::db::Database;

/// 创建快捷按钮。调用方需先用 get_quick_button_by_name 预检重名（DB 还有 UNIQUE 兜底）。
pub async fn create_quick_button(
    db: &Database,
    workspace_id: i64,
    button_name: &str,
    prompt_text: &str,
) -> Result<i64, sea_orm::DbErr> {
    use crate::db::entity::quick_buttons as qb;

    // created/updated 同步写入，避免后续按 created_at 排序时出现 NULL 导致顺序不定
    let now = crate::models::utc_timestamp();
    let am = qb::ActiveModel {
        button_name: ActiveValue::Set(button_name.to_string()),
        prompt_text: ActiveValue::Set(prompt_text.to_string()),
        workspace_id: ActiveValue::Set(Some(workspace_id)),
        created_at: ActiveValue::Set(Some(now.clone())),
        updated_at: ActiveValue::Set(Some(now)),
        ..Default::default()
    };

    let result = am.insert(&db.conn).await?;
    Ok(result.id)
}

/// 获取指定 workspace 下的全部快捷按钮，按创建时间升序（先加的排前面）。
pub async fn get_quick_buttons(
    db: &Database,
    workspace_id: i64,
) -> Result<Vec<crate::db::entity::quick_buttons::Model>, sea_orm::DbErr> {
    use crate::db::entity::quick_buttons as qb;

    let buttons = qb::Entity::find()
        .filter(qb::Column::WorkspaceId.eq(workspace_id))
        .order_by_asc(qb::Column::CreatedAt)
        .all(&db.conn)
        .await?;

    Ok(buttons)
}

/// 按 workspace + 名称查找按钮（handler 重名预检用）。
pub async fn get_quick_button_by_name(
    db: &Database,
    workspace_id: i64,
    button_name: &str,
) -> Result<Option<crate::db::entity::quick_buttons::Model>, sea_orm::DbErr> {
    use crate::db::entity::quick_buttons as qb;

    let button = qb::Entity::find()
        .filter(qb::Column::WorkspaceId.eq(workspace_id))
        .filter(qb::Column::ButtonName.eq(button_name))
        .one(&db.conn)
        .await?;

    Ok(button)
}

/// 删除快捷按钮（仅在指定 workspace 内）。
pub async fn delete_quick_button(
    db: &Database,
    workspace_id: i64,
    id: i64,
) -> Result<(), sea_orm::DbErr> {
    use crate::db::entity::quick_buttons as qb;

    qb::Entity::delete(
        qb::ActiveModel {
            id: ActiveValue::Set(id),
            workspace_id: ActiveValue::Set(Some(workspace_id)),
            ..Default::default()
        }
        .into_active_model(),
    )
    .exec(&db.conn)
    .await?;
    Ok(())
}

/// 更新快捷按钮（仅在指定 workspace 内）。
/// name/prompt 为 None 表示不改该字段；记录不存在则静默返回。
pub async fn update_quick_button(
    db: &Database,
    workspace_id: i64,
    id: i64,
    button_name: Option<&str>,
    prompt_text: Option<&str>,
) -> Result<(), sea_orm::DbErr> {
    use crate::db::entity::quick_buttons as qb;

    let model = qb::Entity::find_by_id(id)
        .filter(qb::Column::WorkspaceId.eq(workspace_id))
        .one(&db.conn)
        .await?;
    let Some(model) = model else {
        // 记录不存在或不在该 workspace 视作成功，避免 update 报错冒泡
        return Ok(());
    };

    let mut am = model.into_active_model();
    if let Some(name) = button_name {
        am.button_name = ActiveValue::Set(name.to_string());
    }
    if let Some(prompt) = prompt_text {
        am.prompt_text = ActiveValue::Set(prompt.to_string());
    }

    am.updated_at = ActiveValue::Set(Some(crate::models::utc_timestamp()));
    am.update(&db.conn).await?;
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
mod quick_button_tests {
    use super::*;

    async fn fresh_db() -> Database {
        Database::new(":memory:").await.expect("memory db must open")
    }

    /// create_quick_button：插入后返回正数 id，且能被列出。
    #[tokio::test]
    async fn test_create_quick_button_inserts_record() {
        let db = fresh_db().await;
        let id = create_quick_button(&db, 1, "提取skill", "话术").await.unwrap();
        assert!(id > 0, "应返回正数 id");
        let list = get_quick_buttons(&db, 1).await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].button_name, "提取skill");
    }

    /// get_quick_buttons：按 created_at 升序，先创建的排前面。
    #[tokio::test]
    async fn test_get_quick_buttons_returns_ascending_by_created() {
        let db = fresh_db().await;
        create_quick_button(&db, 1, "第一个", "a").await.unwrap();
        create_quick_button(&db, 1, "第二个", "b").await.unwrap();
        let list = get_quick_buttons(&db, 1).await.unwrap();
        assert_eq!(list[0].button_name, "第一个");
        assert_eq!(list[1].button_name, "第二个");
    }

    /// get_quick_button_by_name：已存在返回 Some，不存在返回 None。
    #[tokio::test]
    async fn test_get_quick_button_by_name_finds_existing() {
        let db = fresh_db().await;
        create_quick_button(&db, 1, "提取skill", "话术").await.unwrap();
        assert!(get_quick_button_by_name(&db, 1, "提取skill").await.unwrap().is_some());
        assert!(get_quick_button_by_name(&db, 1, "不存在").await.unwrap().is_none());
    }

    /// create_quick_button：同名重复插入触发 UNIQUE(button_name) 约束报错（DB 兜底）。
    #[tokio::test]
    async fn test_create_quick_button_duplicate_name_errors() {
        let db = fresh_db().await;
        create_quick_button(&db, 1, "提取skill", "话术").await.unwrap();
        // 第二次同名 insert 应被唯一约束拒绝
        let err = create_quick_button(&db, 1, "提取skill", "话术2").await;
        assert!(err.is_err(), "重名 insert 应返回 DbErr");
    }

    /// update_quick_button：同时改名改话术，新值生效、id 不变。
    #[tokio::test]
    async fn test_update_quick_button_changes_fields() {
        let db = fresh_db().await;
        let id = create_quick_button(&db, 1, "提取skill", "旧话术").await.unwrap();
        update_quick_button(&db, 1, id, Some("提取SKILL"), Some("新话术"))
            .await
            .unwrap();
        let updated = get_quick_button_by_name(&db, 1, "提取SKILL")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(updated.prompt_text, "新话术", "话术应已更新");
        assert_eq!(updated.id, id, "id 不变");
    }

    /// update_quick_button：只传 name（prompt 为 None）时话术保持不变（局部更新）。
    #[tokio::test]
    async fn test_update_quick_button_partial_only_name() {
        let db = fresh_db().await;
        let id = create_quick_button(&db, 1, "提取skill", "原话术").await.unwrap();
        update_quick_button(&db, 1, id, Some("改名"), None).await.unwrap();
        let updated = get_quick_button_by_name(&db, 1, "改名")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(updated.prompt_text, "原话术", "未传 prompt 时话术应保持原值");
    }

    /// update_quick_button：记录不存在时静默返回 Ok（幂等语义，避免 update 报错冒泡）。
    #[tokio::test]
    async fn test_update_quick_button_silent_when_missing() {
        let db = fresh_db().await;
        let result = update_quick_button(&db, 1, 99999, Some("x"), Some("y")).await;
        assert!(result.is_ok(), "更新不存在的记录应静默 Ok");
        assert!(get_quick_buttons(&db, 1).await.unwrap().is_empty(), "不应产生新记录");
    }

    /// delete_quick_button：删除后列表为空。
    #[tokio::test]
    async fn test_delete_quick_button_removes_record() {
        let db = fresh_db().await;
        let id = create_quick_button(&db, 1, "提取skill", "话术").await.unwrap();
        delete_quick_button(&db, 1, id).await.unwrap();
        assert!(get_quick_buttons(&db, 1).await.unwrap().is_empty(), "删除后应无记录");
    }
}
