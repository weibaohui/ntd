//! Usage statistics database operations.
//!
//! Contains all CRUD operations for usage tracking:
//! - Daily usage stats (per date + type)
//! - Model breakdown stats (per daily stat)
//! - Executor daily stats (per date + executor)

use sea_orm::{ColumnTrait, ConnectionTrait, DbBackend, EntityTrait, Order, QueryFilter, QueryOrder, Statement};

use super::{Database, ModelBreakdownWithDate};

impl Database {
    /// Create a new usage daily stat record.
    pub async fn create_usage_daily_stat(
        &self,
        date: &str,
        project_path: Option<&str>,
        session_id: Option<&str>,
        input_tokens: i64,
        output_tokens: i64,
        cache_creation_tokens: i64,
        cache_read_tokens: i64,
        extra_total_tokens: i64,
        total_cost: f64,
        credits: Option<f64>,
        message_count: Option<i64>,
        models_used: &[String],
        project: Option<&str>,
        versions: Option<&[String]>,
        last_activity: Option<&str>,
        stats_type: &str,
    ) -> Result<i64, sea_orm::DbErr> {
        use crate::db::entity::usage_stats;
        use sea_orm::ActiveValue::Set;

        let models_used_json = serde_json::to_string(models_used).unwrap_or_else(|_| "[]".to_string());
        let versions_json = versions.and_then(|v| serde_json::to_string(v).ok());

        let active_model = usage_stats::ActiveModel {
            date: Set(date.to_string()),
            project_path: Set(project_path.map(|s| s.to_string())),
            session_id: Set(session_id.map(|s| s.to_string())),
            input_tokens: Set(input_tokens),
            output_tokens: Set(output_tokens),
            cache_creation_tokens: Set(cache_creation_tokens),
            cache_read_tokens: Set(cache_read_tokens),
            extra_total_tokens: Set(extra_total_tokens),
            total_cost: Set(total_cost),
            credits: Set(credits),
            message_count: Set(message_count),
            models_used: Set(models_used_json),
            project: Set(project.map(|s| s.to_string())),
            versions: Set(versions_json),
            last_activity: Set(last_activity.map(|s| s.to_string())),
            stats_type: Set(stats_type.to_string()),
            ..Default::default()
        };

        let result = usage_stats::Entity::insert(active_model)
            .exec(&self.conn)
            .await?;

        Ok(result.last_insert_id)
    }

    /// Create a model breakdown record.
    pub async fn create_usage_model_breakdown(
        &self,
        daily_stat_id: i64,
        model_name: &str,
        input_tokens: i64,
        output_tokens: i64,
        cache_creation_tokens: i64,
        cache_read_tokens: i64,
        extra_total_tokens: i64,
        cost: f64,
    ) -> Result<i64, sea_orm::DbErr> {
        use crate::db::entity::usage_model_breakdown;
        use sea_orm::ActiveValue::Set;

        let active_model = usage_model_breakdown::ActiveModel {
            daily_stat_id: Set(daily_stat_id),
            model_name: Set(model_name.to_string()),
            input_tokens: Set(input_tokens),
            output_tokens: Set(output_tokens),
            cache_creation_tokens: Set(cache_creation_tokens),
            cache_read_tokens: Set(cache_read_tokens),
            extra_total_tokens: Set(extra_total_tokens),
            cost: Set(cost),
            ..Default::default()
        };

        let result = usage_model_breakdown::Entity::insert(active_model)
            .exec(&self.conn)
            .await?;

        Ok(result.last_insert_id)
    }

    /// Get usage stats by type and date range.
    pub async fn get_usage_stats(
        &self,
        stats_type: &str,
        since: Option<&str>,
        until: Option<&str>,
    ) -> Result<Vec<crate::db::entity::usage_stats::Model>, sea_orm::DbErr> {
        use crate::db::entity::usage_stats;

        let mut query = usage_stats::Entity::find();
        query = query.filter(usage_stats::Column::StatsType.eq(stats_type));

        if let Some(since_date) = since {
            query = query.filter(usage_stats::Column::Date.gte(since_date));
        }
        if let Some(until_date) = until {
            query = query.filter(usage_stats::Column::Date.lte(until_date));
        }

        let results = query
            .order_by(usage_stats::Column::Date, Order::Desc)
            .all(&self.conn)
            .await?;

        Ok(results)
    }

    /// Get model breakdowns for a specific daily stat.
    pub async fn get_usage_model_breakdowns(
        &self,
        daily_stat_id: i64,
    ) -> Result<Vec<crate::db::entity::usage_model_breakdown::Model>, sea_orm::DbErr> {
        use crate::db::entity::usage_model_breakdown;

        let results = usage_model_breakdown::Entity::find()
            .filter(usage_model_breakdown::Column::DailyStatId.eq(daily_stat_id))
            .all(&self.conn)
            .await?;

        Ok(results)
    }

    /// Get model breakdowns for a date range (via join with daily_stats).
    pub async fn get_usage_model_breakdowns_by_date_range(
        &self,
        stats_type: &str,
        since: Option<&str>,
        until: Option<&str>,
    ) -> Result<Vec<ModelBreakdownWithDate>, sea_orm::DbErr> {
        let daily_stats = self.get_usage_stats(stats_type, since, until).await?;

        if daily_stats.is_empty() {
            return Ok(vec![]);
        }

        let stat_ids: Vec<i64> = daily_stats.iter().map(|s| s.id).collect();
        let stat_dates: std::collections::HashMap<i64, String> = daily_stats
            .iter()
            .map(|s| (s.id, s.date.clone()))
            .collect();

        let mut all_breakdowns: Vec<ModelBreakdownWithDate> = vec![];

        for stat_id in stat_ids {
            let breakdowns = self.get_usage_model_breakdowns(stat_id).await?;
            let date = stat_dates.get(&stat_id).cloned().unwrap_or_default();
            for bd in breakdowns {
                all_breakdowns.push(ModelBreakdownWithDate {
                    date: date.clone(),
                    model_name: bd.model_name,
                    input_tokens: bd.input_tokens,
                    output_tokens: bd.output_tokens,
                    cache_creation_tokens: bd.cache_creation_tokens,
                    cache_read_tokens: bd.cache_read_tokens,
                    extra_total_tokens: bd.extra_total_tokens,
                    cost: bd.cost,
                });
            }
        }

        Ok(all_breakdowns)
    }

    /// Delete existing stats for a specific date and type (for re-computation).
    pub async fn delete_usage_stats_by_date(
        &self,
        date: &str,
        stats_type: &str,
    ) -> Result<(), sea_orm::DbErr> {
        use crate::db::entity::usage_stats;
        use sea_orm::Delete;

        let daily_stats: Vec<usage_stats::Model> = usage_stats::Entity::find()
            .filter(usage_stats::Column::Date.eq(date))
            .filter(usage_stats::Column::StatsType.eq(stats_type))
            .all(&self.conn)
            .await?;

        for stat in daily_stats {
            Delete::one(stat).exec(&self.conn).await?;
        }

        Delete::many(usage_stats::Entity)
            .filter(usage_stats::Column::Date.eq(date))
            .filter(usage_stats::Column::StatsType.eq(stats_type))
            .exec(&self.conn)
            .await?;

        Ok(())
    }

    /// Get the most recent stat for a specific date and type.
    pub async fn get_latest_usage_stat(
        &self,
        date: &str,
        stats_type: &str,
    ) -> Result<Option<crate::db::entity::usage_stats::Model>, sea_orm::DbErr> {
        use crate::db::entity::usage_stats;

        let result = usage_stats::Entity::find()
            .filter(usage_stats::Column::Date.eq(date))
            .filter(usage_stats::Column::StatsType.eq(stats_type))
            .one(&self.conn)
            .await?;

        Ok(result)
    }

    /// Create or update usage executor daily stat record.
    pub async fn upsert_usage_executor_daily_stat(
        &self,
        date: &str,
        executor: &str,
        input_tokens: i64,
        output_tokens: i64,
        cache_creation_tokens: i64,
        cache_read_tokens: i64,
        extra_total_tokens: i64,
        total_cost: f64,
        credits: Option<f64>,
        message_count: Option<i64>,
        model: Option<&str>,
        execution_count: i64,
    ) -> Result<i64, sea_orm::DbErr> {
        let sql = r#"INSERT INTO usage_executor_daily_stats
               (date, executor, input_tokens, output_tokens, cache_creation_tokens,
                cache_read_tokens, extra_total_tokens, total_cost, credits, message_count,
                model, execution_count)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
               ON CONFLICT(date, executor) DO UPDATE SET
                input_tokens = input_tokens + excluded.input_tokens,
                output_tokens = output_tokens + excluded.output_tokens,
                cache_creation_tokens = cache_creation_tokens + excluded.cache_creation_tokens,
                cache_read_tokens = cache_read_tokens + excluded.cache_read_tokens,
                extra_total_tokens = extra_total_tokens + excluded.extra_total_tokens,
                total_cost = total_cost + excluded.total_cost,
                credits = COALESCE(credits, 0) + COALESCE(excluded.credits, 0),
                message_count = COALESCE(message_count, 0) + COALESCE(excluded.message_count, 0),
                model = excluded.model,
                execution_count = execution_count + excluded.execution_count"#;

        let stmt = Statement::from_sql_and_values(
            DbBackend::Sqlite,
            sql,
            vec![
                date.into(),
                executor.into(),
                input_tokens.into(),
                output_tokens.into(),
                cache_creation_tokens.into(),
                cache_read_tokens.into(),
                extra_total_tokens.into(),
                total_cost.into(),
                credits.into(),
                message_count.into(),
                model.into(),
                execution_count.into(),
            ],
        );

        let result = self.conn.execute(stmt).await?;
        Ok(result.last_insert_id() as i64)
    }

    /// Get usage executor daily stats by date range.
    pub async fn get_usage_executor_daily_stats(
        &self,
        since: Option<&str>,
        until: Option<&str>,
    ) -> Result<Vec<crate::db::entity::usage_executor_daily::Model>, sea_orm::DbErr> {
        use crate::db::entity::usage_executor_daily;

        let mut query = usage_executor_daily::Entity::find();

        if let Some(since_date) = since {
            query = query.filter(usage_executor_daily::Column::Date.gte(since_date));
        }
        if let Some(until_date) = until {
            query = query.filter(usage_executor_daily::Column::Date.lte(until_date));
        }

        let results = query
            .order_by_desc(usage_executor_daily::Column::Date)
            .order_by_asc(usage_executor_daily::Column::Executor)
            .all(&self.conn)
            .await?;

        Ok(results)
    }

    /// Delete usage executor stats for a specific date.
    pub async fn delete_usage_executor_stats_by_date(&self, date: &str) -> Result<(), sea_orm::DbErr> {
        use crate::db::entity::usage_executor_daily;
        use sea_orm::Delete;

        Delete::many(usage_executor_daily::Entity)
            .filter(usage_executor_daily::Column::Date.eq(date))
            .exec(&self.conn)
            .await?;

        Ok(())
    }
}
