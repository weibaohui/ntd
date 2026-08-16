//! 工作空间设置的数据库访问层
//!
//! 提供 workspace_settings 表的 CRUD 操作。
//! 108 空间管家：默认响应四字段（default_response_*）已随 V95 退役，
//! 管家配置由 butler_expert_name / butler_executor 两字段承载。

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
/// 空串 `Some("")` 表示显式清空（管家两字段与 `system_prompt` 同口径），
/// 下游读取方把空串与 NULL 同等视为「未配置」，不再保留旧 loop_id=0 的哨兵特例。
pub async fn upsert_workspace_settings(
    db: &Database,
    workspace_id: i64,
    butler_expert_name: Option<String>,
    butler_executor: Option<String>,
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
        if let Some(name) = butler_expert_name {
            am.butler_expert_name = ActiveValue::Set(Some(name));
        }
        if let Some(exec) = butler_executor {
            am.butler_executor = ActiveValue::Set(Some(exec));
        }
        // system_prompt：Some(含空串) 覆写，None 不动
        if let Some(prompt) = system_prompt {
            am.system_prompt = ActiveValue::Set(Some(prompt));
        }
        am.updated_at = ActiveValue::Set(Some(crate::models::utc_timestamp()));
        am.update(&db.conn).await?;
    } else {
        // 创建：None 字段落 NULL（=未配置），108 起不再有任何「默认响应类型」默认值
        let now = crate::models::utc_timestamp();
        let am = ws::ActiveModel {
            // 新建记录时主键由 DB 自增，显式标记 NotSet
            id: ActiveValue::NotSet,
            workspace_id: ActiveValue::Set(workspace_id),
            butler_expert_name: ActiveValue::Set(butler_expert_name),
            butler_executor: ActiveValue::Set(butler_executor),
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
/// 刻意独立于 [`upsert_workspace_settings`]：后者是多字段位置式签名，既有调用点多数与本字段
/// 无关（executor_service 的 prompt 注入流程），强加新参会迫使它们各补一个 `None`，
/// 既增加无关 diff、也放大冲突面。单字段更新走本函数，零波及既有调用，且「接力护栏」与
/// 「管家配置」本就是两个关注点，分而治之更清晰。
///
/// 语义（与任务级 [`crate::db::task::update_delegate_max_rounds`] 一致，便于记忆）：
/// - `Some(n)`（n≥1，由 handler 校验）→ 置为 n，任务级未覆盖时以此为准。
/// - `None` → 清空该列，回退终极兜底常量 `MAX_DELEGATE_ROUNDS`。
///
/// 行不存在时按 [`upsert_workspace_settings`] 同口径新建一行，仅本字段带值，其余落 NULL。
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
            butler_expert_name: ActiveValue::Set(None),
            butler_executor: ActiveValue::Set(None),
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
        upsert_workspace_settings(&db, 1, None, None, Some(prompt.to_string()))
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
        upsert_workspace_settings(&db, 1, None, None, Some(prompt.to_string()))
            .await
            .unwrap();
        // 第二次更新管家执行器，system_prompt 传 None
        upsert_workspace_settings(&db, 1, None, Some("pi".to_string()), None)
            .await
            .unwrap();
        let settings = get_workspace_settings(&db, 1).await.unwrap().unwrap();
        assert_eq!(settings.system_prompt.as_deref(), Some(prompt));
        assert_eq!(settings.butler_executor.as_deref(), Some("pi"));
    }

    /// 显式传空串 Some("") 覆写原 prompt。
    #[tokio::test]
    async fn test_upsert_empty_string_clears_prompt() {
        let db = Database::new(":memory:").await.unwrap();
        // 先写入非空 prompt
        upsert_workspace_settings(&db, 1, None, None, Some("共识".to_string()))
            .await
            .unwrap();
        // 显式传空串清空
        upsert_workspace_settings(&db, 1, None, None, Some(String::new()))
            .await
            .unwrap();
        let settings = get_workspace_settings(&db, 1).await.unwrap().unwrap();
        assert_eq!(settings.system_prompt.as_deref(), Some(""));
    }

    /// 管家两字段：写入后读取一致；传 None 不误伤已有值；空串可显式清空专家。
    #[tokio::test]
    async fn test_upsert_butler_fields_roundtrip_and_clear() {
        let db = Database::new(":memory:").await.unwrap();
        // 写入管家配置
        upsert_workspace_settings(
            &db,
            1,
            Some("workspace-butler".to_string()),
            Some("claudecode".to_string()),
            None,
        )
        .await
        .unwrap();
        let s = get_workspace_settings(&db, 1).await.unwrap().unwrap();
        assert_eq!(s.butler_expert_name.as_deref(), Some("workspace-butler"));
        assert_eq!(s.butler_executor.as_deref(), Some("claudecode"));
        // 只更新执行器，专家传 None → 专家保持原值
        upsert_workspace_settings(&db, 1, None, Some("pi".to_string()), None)
            .await
            .unwrap();
        let s = get_workspace_settings(&db, 1).await.unwrap().unwrap();
        assert_eq!(s.butler_expert_name.as_deref(), Some("workspace-butler"));
        assert_eq!(s.butler_executor.as_deref(), Some("pi"));
        // 空串显式清空专家（下游按「空=未配置」处理），执行器不动
        upsert_workspace_settings(&db, 1, Some(String::new()), None, None)
            .await
            .unwrap();
        let s = get_workspace_settings(&db, 1).await.unwrap().unwrap();
        assert_eq!(s.butler_expert_name.as_deref(), Some(""));
        assert_eq!(s.butler_executor.as_deref(), Some("pi"));
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
        // 新建行管家字段落 NULL（=未配置），108 起不再有默认响应类型默认值。
        assert_eq!(s.butler_executor, None);
        assert_eq!(s.butler_expert_name, None);
    }

    /// 既有行：置值后再传 None 清空（回退兜底），不影响其他列。
    #[tokio::test]
    async fn test_update_workspace_delegate_max_rounds_set_then_clear() {
        let db = Database::new(":memory:").await.unwrap();
        // 先建行并写入 system_prompt，验证单字段更新不误伤其他列。
        upsert_workspace_settings(&db, 1, None, None, Some("共识".to_string()))
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
