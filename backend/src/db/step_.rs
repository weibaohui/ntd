use sea_orm::{
    ColumnTrait, ConnectionTrait, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder,
    QuerySelect, Statement,
};

use crate::db::entity::steps;
use crate::db::Database;

impl Database {
    /// 列出所有环节（按 id 倒序）。
    pub async fn list_steps_pure(&self) -> Result<Vec<steps::Model>, sea_orm::DbErr> {
        steps::Entity::find()
            .order_by_desc(steps::Column::Id)
            .all(&self.conn)
            .await
    }

    /// 单个环节详情。
    pub async fn get_step(&self, id: i64) -> Result<Option<steps::Model>, sea_orm::DbErr> {
        steps::Entity::find_by_id(id).one(&self.conn).await
    }

    /// 创建环节（从 todo 复制数据）。
    pub async fn create_step(
        &self,
        title: &str,
        prompt: &str,
        executor: Option<&str>,
        acceptance_criteria: Option<&str>,
        source_todo_id: Option<i64>,
        color: Option<&str>,
    ) -> Result<steps::Model, sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let c = color.unwrap_or("#722ed1");
        let sql = "INSERT INTO steps (title, prompt, executor, acceptance_criteria, source_todo_id, color, created_at, updated_at) \
                   VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)";
        self.conn
            .execute(Statement::from_sql_and_values(
                sea_orm::DbBackend::Sqlite,
                sql,
                [
                    title.to_string().into(),
                    prompt.to_string().into(),
                    executor.map(|s| s.to_string()).into(),
                    acceptance_criteria.map(|s| s.to_string()).into(),
                    source_todo_id.into(),
                    c.to_string().into(),
                    now.clone().into(),
                    now.into(),
                ],
            ))
            .await?;

        // 查回刚插入的行（last_insert_rowid 在多线程下不可靠，用 order desc + limit 1）
        Ok(steps::Entity::find()
            .order_by_desc(steps::Column::Id)
            .one(&self.conn)
            .await?
            .expect("freshly inserted step should exist"))
    }

    /// 统计某个 step 被多少 loop stage 引用。
    pub async fn count_loop_stages_using_step(
        &self,
        step_id: i64,
    ) -> Result<i64, sea_orm::DbErr> {
        use crate::db::entity::loop_stages;
        Ok(loop_stages::Entity::find()
            .filter(loop_stages::Column::TodoId.eq(step_id))
            .count(&self.conn)
            .await? as i64)
    }

    /// 批量统计多个 step 的引用计数。
    pub async fn count_loop_stages_for_steps(
        &self,
        step_ids: &[i64],
    ) -> Result<std::collections::HashMap<i64, i64>, sea_orm::DbErr> {
        use crate::db::entity::loop_stages;
        let mut map = std::collections::HashMap::new();
        for id in step_ids {
            let count = loop_stages::Entity::find()
                .filter(loop_stages::Column::TodoId.eq(*id))
                .count(&self.conn)
                .await? as i64;
            map.insert(*id, count);
        }
        Ok(map)
    }

    /// 列出环节 + 引用计数（供列表页使用）。
    pub async fn list_steps_with_usage_pure(&self) -> Result<Vec<(steps::Model, i64)>, sea_orm::DbErr> {
        let items = self.list_steps_pure().await?;
        let ids: Vec<i64> = items.iter().map(|s| s.id).collect();
        let usage = self.count_loop_stages_for_steps(&ids).await?;
        Ok(items
            .into_iter()
            .map(|s| {
                let count = usage.get(&s.id).copied().unwrap_or(0);
                (s, count)
            })
            .collect())
    }

    /// 更新环节基本信息。
    pub async fn update_step(
        &self,
        id: i64,
        title: &str,
        prompt: &str,
        executor: Option<&str>,
        acceptance_criteria: Option<&str>,
        color: Option<&str>,
    ) -> Result<(), sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let sql = if let Some(c) = color {
            "UPDATE steps SET title = ?1, prompt = ?2, executor = ?3, acceptance_criteria = ?4, color = ?5, updated_at = ?6 WHERE id = ?7"
        } else {
            "UPDATE steps SET title = ?1, prompt = ?2, executor = ?3, acceptance_criteria = ?4, updated_at = ?5 WHERE id = ?6"
        };
        let vals: Vec<sea_orm::Value> = if let Some(c) = color {
            vec![
                title.to_string().into(),
                prompt.to_string().into(),
                executor.map(|s| s.to_string()).into(),
                acceptance_criteria.map(|s| s.to_string()).into(),
                c.to_string().into(),
                now.into(),
                id.into(),
            ]
        } else {
            vec![
                title.to_string().into(),
                prompt.to_string().into(),
                executor.map(|s| s.to_string()).into(),
                acceptance_criteria.map(|s| s.to_string()).into(),
                now.into(),
                id.into(),
            ]
        };
        self.conn
            .execute(sea_orm::Statement::from_sql_and_values(
                sea_orm::DbBackend::Sqlite,
                sql,
                vals,
            ))
            .await?;
        Ok(())
    }
}
