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

impl Database {
    /// 批量取每个 todo 绑定的工作空间斜杠命令（command_type='todo' 且 enabled=1）。
    ///
    /// 事项中心卡片展示「绑定命令: /xxx」用。一个 todo 可能被多条命令绑定，
    /// 取 MIN(slash_command) 保证输出确定（字典序最小的一条）。
    /// 返回 `todo_id -> 命令串`，未绑定的 todo 不在 map 中。
    pub async fn get_bound_slash_commands_for_todos(
        &self,
        todo_ids: &[i64],
    ) -> Result<std::collections::HashMap<i64, String>, sea_orm::DbErr> {
        if todo_ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }
        let (placeholders, values) = Database::in_clause(todo_ids);
        let sql = format!(
            "SELECT todo_id, MIN(slash_command) AS slash_command FROM workspace_slash_commands \
             WHERE command_type='todo' AND enabled=1 AND todo_id IN ({placeholders}) \
             GROUP BY todo_id"
        );
        let rows = self.query_all_sql(sql, values).await?;
        let mut map = std::collections::HashMap::new();
        for row in rows {
            let todo_id: i64 = row.try_get_by("todo_id")?;
            let cmd: String = row.try_get_by("slash_command")?;
            map.insert(todo_id, cmd);
        }
        Ok(map)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
mod slash_command_batch_tests {
    use super::*;
    use sea_orm::ConnectionTrait;

    async fn fresh_db() -> Database {
        Database::new(":memory:").await.expect("memory db must open")
    }

    /// 插一条 todo 并返回 id（slash_commands.todo_id 有 FK 约束需指向真实 todo）。
    async fn seed_todo(db: &Database, title: &str) -> i64 {
        db.exec(&format!(
            "INSERT INTO todos (title, prompt, status) VALUES ('{title}', 'p', 'pending')"
        ))
        .await
        .expect("insert todo");
        let row = db
            .conn
            .query_one(sea_orm::Statement::from_string(
                sea_orm::DbBackend::Sqlite,
                format!("SELECT id FROM todos WHERE title = '{title}'"),
            ))
            .await
            .expect("query id")
            .expect("row exists");
        row.try_get_by_index::<i64>(0).expect("id readable")
    }

    /// 插一条 slash command 绑定 todo。
    async fn seed_cmd(db: &Database, workspace_id: i64, cmd: &str, todo_id: i64, enabled: bool) {
        let en = if enabled { 1 } else { 0 };
        db.exec(&format!(
            "INSERT INTO workspace_slash_commands (workspace_id, slash_command, command_type, todo_id, enabled) \
             VALUES ({workspace_id}, '{cmd}', 'todo', {todo_id}, {en})"
        ))
        .await
        .expect("insert cmd");
    }

    /// get_bound_slash_commands_for_todos：返回每个 todo 绑定的命令（enabled=1，command_type=todo）。
    /// 禁用命令与 loop 类型命令不计；一个 todo 多条命令取 MIN。
    #[tokio::test]
    async fn test_get_bound_slash_commands_for_todos_filters() {
        let db = fresh_db().await;
        let t1 = seed_todo(&db, "T1").await;
        let t2 = seed_todo(&db, "T2").await;
        // t1 绑定 /aaa（启用）
        seed_cmd(&db, 1, "/aaa", t1, true).await;
        // t1 再绑定 /bbb（启用）→ 取 MIN = /aaa
        seed_cmd(&db, 1, "/bbb", t1, true).await;
        // t1 绑定 /ccc（禁用）→ 不计
        seed_cmd(&db, 1, "/ccc", t1, false).await;
        // t2 绑定 /yyy（启用）
        seed_cmd(&db, 1, "/yyy", t2, true).await;

        let map = db.get_bound_slash_commands_for_todos(&[t1, t2]).await.unwrap();
        assert_eq!(map.get(&t1).map(String::as_str), Some("/aaa"), "多条取 MIN");
        assert_eq!(map.get(&t2).map(String::as_str), Some("/yyy"));
    }
}
