use sea_orm::{ConnectionTrait, Statement};

use crate::db::Database;

impl Database {
    // 「调用追踪」tab 已移除，原 get_skill_invocations / get_skill_invocations_count
    // 仅服务于该 tab 的列表接口，整体删除。
    // Dashboard 上的「调用次数 / 成功率」走 db/dashboard.rs 独立聚合路径。
    // 保留：record_skill_invocation（POST /api/skills/invocations 写入），仍被执行器调用上报。

    pub async fn record_skill_invocation(
        &self,
        skill_name: &str,
        executor: &str,
        todo_id: i64,
        status: &str,
        duration_ms: Option<i64>,
    ) -> Result<i64, sea_orm::DbErr> {
        let backend = self.conn.get_database_backend();

        let (sql, params) = if let Some(d) = duration_ms {
            (
                "INSERT INTO skill_invocations (skill_name, executor, todo_id, status, duration_ms) \
                 VALUES ($1, $2, $3, $4, $5) RETURNING id".to_string(),
                vec![skill_name.into(), executor.into(), todo_id.into(), status.into(), d.into()],
            )
        } else {
            (
                "INSERT INTO skill_invocations (skill_name, executor, todo_id, status) \
                 VALUES ($1, $2, $3, $4) RETURNING id".to_string(),
                vec![skill_name.into(), executor.into(), todo_id.into(), status.into()],
            )
        };

        let result = self.conn.query_one(Statement::from_sql_and_values(backend, sql, params)).await?;

        result
            .and_then(|r| r.try_get_by_index(0).ok())
            .flatten()
            .ok_or_else(|| sea_orm::DbErr::Query(sea_orm::RuntimeErr::Internal("Failed to get last_insert_rowid".to_string())))
    }
}
