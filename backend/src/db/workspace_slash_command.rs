//! 工作空间斜杠命令的数据库访问层
//!
//! 提供 workspace_slash_commands 表的 CRUD 操作。

use sea_orm::{ActiveModelTrait, ActiveValue, ColumnTrait, EntityTrait, IntoActiveModel, QueryFilter, QueryOrder};
use crate::db::Database;

/// 创建斜杠命令
pub async fn create_workspace_slash_command(
    db: &Database,
    workspace_id: i64,
    slash_command: &str,
    command_type: &str,
    todo_id: i64,
    loop_id: Option<i64>,
    enabled: bool,
) -> Result<i64, sea_orm::DbErr> {
    use crate::db::entity::workspace_slash_commands as ws_cmd;

    let now = crate::models::utc_timestamp();
    let am = ws_cmd::ActiveModel {
        workspace_id: ActiveValue::Set(workspace_id),
        slash_command: ActiveValue::Set(slash_command.to_string()),
        command_type: ActiveValue::Set(command_type.to_string()),
        todo_id: ActiveValue::Set(todo_id),
        loop_id: ActiveValue::Set(loop_id),
        enabled: ActiveValue::Set(enabled),
        created_at: ActiveValue::Set(Some(now.clone())),
        updated_at: ActiveValue::Set(Some(now)),
        ..Default::default()
    };

    let result = am.insert(&db.conn).await?;
    Ok(result.id)
}

/// 获取工作空间的所有斜杠命令
pub async fn get_workspace_slash_commands(
    db: &Database,
    workspace_id: i64,
) -> Result<Vec<crate::db::entity::workspace_slash_commands::Model>, sea_orm::DbErr> {
    use crate::db::entity::workspace_slash_commands as ws_cmd;

    let commands = ws_cmd::Entity::find()
        .filter(ws_cmd::Column::WorkspaceId.eq(workspace_id))
        .order_by_asc(ws_cmd::Column::SlashCommand)
        .all(&db.conn)
        .await?;

    Ok(commands)
}

/// 获取工作空间的单个斜杠命令
pub async fn get_workspace_slash_command(
    db: &Database,
    workspace_id: i64,
    slash_command: &str,
) -> Result<Option<crate::db::entity::workspace_slash_commands::Model>, sea_orm::DbErr> {
    use crate::db::entity::workspace_slash_commands as ws_cmd;

    let command = ws_cmd::Entity::find()
        .filter(ws_cmd::Column::WorkspaceId.eq(workspace_id))
        .filter(ws_cmd::Column::SlashCommand.eq(slash_command))
        .one(&db.conn)
        .await?;

    Ok(command)
}

/// 删除斜杠命令
pub async fn delete_workspace_slash_command(
    db: &Database,
    id: i64,
) -> Result<(), sea_orm::DbErr> {
    use crate::db::entity::workspace_slash_commands as ws_cmd;

    ws_cmd::Entity::delete_by_id(id).exec(&db.conn).await?;
    Ok(())
}

/// 更新斜杠命令
pub async fn update_workspace_slash_command(
    db: &Database,
    id: i64,
    slash_command: Option<&str>,
    command_type: Option<&str>,
    todo_id: Option<i64>,
    loop_id: Option<i64>,
    enabled: Option<bool>,
) -> Result<(), sea_orm::DbErr> {
    use crate::db::entity::workspace_slash_commands as ws_cmd;

    let model = ws_cmd::Entity::find_by_id(id).one(&db.conn).await?;
    let Some(model) = model else {
        return Ok(());
    };

    let mut am = model.into_active_model();
    if let Some(cmd) = slash_command {
        am.slash_command = ActiveValue::Set(cmd.to_string());
    }
    if let Some(ct) = command_type {
        am.command_type = ActiveValue::Set(ct.to_string());
    }
    if let Some(tid) = todo_id {
        am.todo_id = ActiveValue::Set(tid);
    }
    // loop_id = Some(0) 表示清空，Some(n) where n > 0 表示设置为该 ID
    if let Some(lid) = loop_id {
        if lid == 0 {
            am.loop_id = ActiveValue::Set(None);
        } else {
            am.loop_id = ActiveValue::Set(Some(lid));
        }
    }
    if let Some(enabled) = enabled {
        am.enabled = ActiveValue::Set(enabled);
    }

    am.updated_at = ActiveValue::Set(Some(crate::models::utc_timestamp()));
    am.update(&db.conn).await?;
    Ok(())
}
