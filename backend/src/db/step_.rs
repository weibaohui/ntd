use sea_orm::{
    ColumnTrait, ConnectionTrait, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder,
    Statement,
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

    /// 统计某个 step 被多少 loop step 引用。
    pub async fn count_loop_steps_using_step(
        &self,
        step_id: i64,
    ) -> Result<i64, sea_orm::DbErr> {
        use crate::db::entity::loop_steps;
        Ok(loop_steps::Entity::find()
            .filter(loop_steps::Column::StepId.eq(step_id))
            .count(&self.conn)
            .await? as i64)
    }

    /// 批量统计多个 step 的引用计数（使用单次聚合查询避免 N+1 问题）。
    pub async fn count_loop_steps_for_steps(
        &self,
        step_ids: &[i64],
    ) -> Result<std::collections::HashMap<i64, i64>, sea_orm::DbErr> {
        
        use sea_orm::Statement;
        
        // 早期返回空结果，避免构建空 SQL
        if step_ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }
        
        // 构建 IN 子句的占位符：?1, ?2, ?3, ...
        let placeholders: Vec<String> = (1..=step_ids.len())
            .map(|i| format!("?{}", i))
            .collect();
        let in_clause = placeholders.join(",");
        
        // 单次聚合查询：GROUP BY step_id 一次性获取所有引用计数
        // 注意：DB 列名是 step_id（commit 9590e63 把 stage→step 重命名后），
        // 但 sea_orm entity 字段仍叫 todo_id，raw SQL 必须用真实列名 step_id。
        let sql = format!(
            "SELECT step_id, COUNT(*) as cnt FROM loop_steps WHERE step_id IN ({}) GROUP BY step_id",
            in_clause
        );

        let values: Vec<sea_orm::Value> = step_ids
            .iter()
            .map(|id| (*id).into())
            .collect();

        let stmt = Statement::from_sql_and_values(sea_orm::DbBackend::Sqlite, sql, values);
        let rows = self.conn.query_all(stmt).await?;

        // 从查询结果构建 HashMap
        let mut map = std::collections::HashMap::new();
        for row in rows {
            let step_id: i64 = row.try_get("", "step_id")?;
            let cnt: i64 = row.try_get("", "cnt")?;
            map.insert(step_id, cnt);
        }

        Ok(map)
    }

    /// 列出环节 + 引用计数（供列表页使用）。
    pub async fn list_steps_with_usage_pure(&self) -> Result<Vec<(steps::Model, i64)>, sea_orm::DbErr> {
        let items = self.list_steps_pure().await?;
        let ids: Vec<i64> = items.iter().map(|s| s.id).collect();
        let usage = self.count_loop_steps_for_steps(&ids).await?;
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
        let (sql, vals) = self.build_update_sql(id, title, prompt, executor, acceptance_criteria, color);
        self.conn
            .execute(sea_orm::Statement::from_sql_and_values(
                sea_orm::DbBackend::Sqlite,
                sql,
                vals,
            ))
            .await?;
        Ok(())
    }

    /// 构建 update_step 的 SQL 和参数值
    fn build_update_sql(
        &self,
        id: i64,
        title: &str,
        prompt: &str,
        executor: Option<&str>,
        acceptance_criteria: Option<&str>,
        color: Option<&str>,
    ) -> (&str, Vec<sea_orm::Value>) {
        let now = crate::models::utc_timestamp();
        let sql = if color.is_some() {
            "UPDATE steps SET title = ?1, prompt = ?2, executor = ?3, acceptance_criteria = ?4, color = ?5, updated_at = ?6 WHERE id = ?7"
        } else {
            "UPDATE steps SET title = ?1, prompt = ?2, executor = ?3, acceptance_criteria = ?4, updated_at = ?5 WHERE id = ?6"
        };
        let mut vals: Vec<sea_orm::Value> = vec![
            title.to_string().into(),
            prompt.to_string().into(),
            executor.map(|s| s.to_string()).into(),
            acceptance_criteria.map(|s| s.to_string()).into(),
        ];
        if let Some(c) = color {
            vals.push(c.to_string().into());
        }
        vals.push(now.into());
        vals.push(id.into());
        (sql, vals)
    }

    /// 删除环节（从 steps 表中删除）。
    pub async fn delete_step(&self, id: i64) -> Result<(), sea_orm::DbErr> {
        use crate::db::entity::steps;
        steps::Entity::delete_by_id(id).exec(&self.conn).await?;
        Ok(())
    }
}
