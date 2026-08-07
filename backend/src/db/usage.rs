//! Usage statistics database operations.
//!
//! Contains all CRUD operations for usage tracking:
//! - Daily usage stats (per date + type)
//! - Model breakdown stats (per daily stat)
//! - Executor daily stats (per date + executor)

use sea_orm::{ColumnTrait, EntityTrait, Order, QueryFilter, QueryOrder};

use super::{Database, ModelBreakdownWithDate};

/// 单条 model breakdown 的写入载荷。
/// db 层独立于 service 的 `ModelBreakdown` 定义此结构，避免 db→service 的循环依赖（091 性能优化）。
#[derive(Clone, Debug)]
pub struct ModelBreakdownRow {
    pub model_name: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_creation_tokens: i64,
    pub cache_read_tokens: i64,
    pub extra_total_tokens: i64,
    pub cost: f64,
}

impl Database {
    /// Create a new usage daily stat record.
    /// 数据库列较多，参数数量由 schema 决定，无法进一步合并
    #[allow(clippy::too_many_arguments)]
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
    /// 数据库列较多，参数数量由 schema 决定
    #[allow(clippy::too_many_arguments)]
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

    /// 批量取多个 daily stat 的 model breakdown，按 daily_stat_id 分组返回。
    /// 按日期区间聚合时用一次 IN 查询替代逐 stat 的 N 次往返（消除 N+1，091 性能优化）。
    pub async fn get_usage_model_breakdowns_by_stat_ids(
        &self,
        daily_stat_ids: &[i64],
    ) -> Result<std::collections::HashMap<i64, Vec<crate::db::entity::usage_model_breakdown::Model>>, sea_orm::DbErr> {
        use crate::db::entity::usage_model_breakdown;
        use std::collections::HashMap;

        // 空入参直接返回空 map，避免生成非法的 `IN ()` SQL。
        if daily_stat_ids.is_empty() {
            return Ok(HashMap::new());
        }
        // is_in 需要 owned 的 Vec（SeaORM 泛型约束），克隆一份避免借用纠缠。
        let ids: Vec<i64> = daily_stat_ids.to_vec();
        let rows = usage_model_breakdown::Entity::find()
            .filter(usage_model_breakdown::Column::DailyStatId.is_in(ids))
            .all(&self.conn)
            .await?;
        // 按 daily_stat_id 分组，保留每个 stat 下属它的 breakdown 列表。
        let mut map: HashMap<i64, Vec<_>> = HashMap::new();
        for row in rows {
            map.entry(row.daily_stat_id).or_default().push(row);
        }
        Ok(map)
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

        // daily_stats 已由 get_usage_stats 按 date DESC 排序；直接按其顺序取 id，
        // 保证返回的 all_breakdowns 仍是「按日期降序」——早期实现用 HashMap keys() 生成
        // stat_ids，迭代顺序不确定，会让按数组顺序渲染图表/表格的调用方顺序抖动（091 评审修复）。
        let stat_ids: Vec<i64> = daily_stats.iter().map(|s| s.id).collect();

        // 一次 IN 查询取回全部 breakdown 并按 stat 分组，避免逐 stat N 次往返。
        let breakdowns_by_stat = self.get_usage_model_breakdowns_by_stat_ids(&stat_ids).await?;

        let mut all_breakdowns: Vec<ModelBreakdownWithDate> = vec![];
        for stat in &daily_stats {
            // 该 stat 没有 breakdown 时跳过（map 未命中），避免无谓的空迭代。
            let Some(breakdowns) = breakdowns_by_stat.get(&stat.id) else {
                continue;
            };
            for bd in breakdowns {
                // bd 取自 map 的引用，model_name 是 String 需 clone；token/cost 为 Copy 直接取。
                all_breakdowns.push(ModelBreakdownWithDate {
                    date: stat.date.clone(),
                    model_name: bd.model_name.clone(),
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

        // 单条批量 DELETE 删除指定日期+类型的统计行。
        // 历史实现先 SELECT 全部行再逐行 `Delete::one`，紧接着又用同条件 `Delete::many` 删一遍：
        // 逐行循环是纯写放大（N 次 round-trip + N 次抢 SQLite 写锁），且后面的 many 已覆盖全部目标行，
        // 故直接用批量删除一条搞定（091 性能优化）。
        Delete::many(usage_stats::Entity)
            .filter(usage_stats::Column::Date.eq(date))
            .filter(usage_stats::Column::StatsType.eq(stats_type))
            .exec(&self.conn)
            .await?;

        Ok(())
    }

    /// 在单个事务内完成「按日期删旧 daily stat → 写入新 daily stat → 批量写入 model breakdown」。
    ///
    /// 取代 service 层原先的「先 SELECT 判存在 → 删 → 插 daily → 逐 model 插 breakdown」流程：
    /// - 删旧改为无条件执行（DELETE 幂等），省掉一次判断往返；
    /// - 多条 breakdown 用一次 `insert_many` 写入，消除逐 model 的 N 次 INSERT 往返；
    /// - 删旧/写新收敛进同一事务，保证统计重算期间不会被并发读出半新半旧的数据（091 性能优化）。
    ///
    /// 入参字段集合镜像 `create_usage_daily_stat` 中由 daily 聚合路径实际写入的子集。
    /// 数据库列较多，参数数量由 schema 决定，无法进一步合并。
    #[allow(clippy::too_many_arguments)]
    pub async fn replace_daily_stats_for_date(
        &self,
        date: &str,
        project_path: Option<&str>,
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
        last_activity: Option<&str>,
        breakdowns: &[ModelBreakdownRow],
    ) -> Result<i64, sea_orm::DbErr> {
        use crate::db::entity::{usage_model_breakdown, usage_stats};
        use sea_orm::ActiveValue::Set;
        use sea_orm::Delete;
        use sea_orm::TransactionTrait;

        // 开启事务：后续删/插/批量插全部走 &txn，任一步失败整体回滚。
        let txn = self.conn.begin().await?;

        // 删旧：无条件删除该 date+daily 的旧行（不存在时 DELETE 影响 0 行，天然幂等）。
        Delete::many(usage_stats::Entity)
            .filter(usage_stats::Column::Date.eq(date))
            .filter(usage_stats::Column::StatsType.eq("daily"))
            .exec(&txn)
            .await?;

        // 构造新 daily stat：models_used/versions 走 JSON 序列化落库。
        let models_used_json =
            serde_json::to_string(models_used).unwrap_or_else(|_| "[]".to_string());
        let active_model = usage_stats::ActiveModel {
            date: Set(date.to_string()),
            project_path: Set(project_path.map(|s| s.to_string())),
            session_id: Set(None),
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
            versions: Set(None),
            last_activity: Set(last_activity.map(|s| s.to_string())),
            stats_type: Set("daily".to_string()),
            ..Default::default()
        };
        let result = usage_stats::Entity::insert(active_model)
            .exec(&txn)
            .await?;
        // breakdown 需引用新 daily stat 的自增 id，必须在 insert 之后取。
        let stat_id = result.last_insert_id;

        // 批量写 breakdown：非空时一次 INSERT 多行；空则跳过（insert_many 空集会生成非法 SQL）。
        if !breakdowns.is_empty() {
            let rows: Vec<usage_model_breakdown::ActiveModel> = breakdowns
                .iter()
                .map(|b| usage_model_breakdown::ActiveModel {
                    daily_stat_id: Set(stat_id),
                    model_name: Set(b.model_name.clone()),
                    input_tokens: Set(b.input_tokens),
                    output_tokens: Set(b.output_tokens),
                    cache_creation_tokens: Set(b.cache_creation_tokens),
                    cache_read_tokens: Set(b.cache_read_tokens),
                    extra_total_tokens: Set(b.extra_total_tokens),
                    cost: Set(b.cost),
                    ..Default::default()
                })
                .collect();
            usage_model_breakdown::Entity::insert_many(rows)
                .exec(&txn)
                .await?;
        }

        // 提交事务：至此删旧+写新对并发读者一次性可见。
        txn.commit().await?;
        Ok(stat_id)
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

}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::db::Database;

    async fn fresh_db() -> Database {
        Database::new(":memory:").await.expect("memory db must open")
    }

    /// replace_daily_stats_for_date：单事务「删旧 → 写 daily → 批量写 breakdown」。
    /// 重复调用同日期应替换而非累加（删旧 + FK 级联清旧 breakdown），breakdown 一次写入多条。
    #[tokio::test]
    async fn test_replace_daily_stats_for_date_replaces_and_batches_breakdowns() {
        let db = fresh_db().await;
        let breakdowns = vec![
            ModelBreakdownRow {
                model_name: "claude".into(),
                input_tokens: 100,
                output_tokens: 50,
                cache_creation_tokens: 0,
                cache_read_tokens: 10,
                extra_total_tokens: 0,
                cost: 0.01,
            },
            ModelBreakdownRow {
                model_name: "gpt".into(),
                input_tokens: 200,
                output_tokens: 20,
                cache_creation_tokens: 5,
                cache_read_tokens: 0,
                extra_total_tokens: 0,
                cost: 0.02,
            },
        ];
        // 首次写入：daily + 2 条 breakdown。
        let stat_id = db
            .replace_daily_stats_for_date(
                "2026-01-01", None, 300, 70, 5, 10, 0, 0.03, None, None,
                &["claude".to_string(), "gpt".to_string()], None, None, &breakdowns,
            )
            .await
            .unwrap();
        let stats = db.get_usage_stats("daily", None, None).await.unwrap();
        assert_eq!(stats.len(), 1, "应仅 1 条 daily 行");
        assert_eq!(stats[0].id, stat_id);
        assert_eq!(stats[0].input_tokens, 300);
        // breakdown 一次批量写入 2 条。
        assert_eq!(
            db.get_usage_model_breakdowns(stat_id).await.unwrap().len(),
            2,
            "应一次写入 2 条 breakdown"
        );

        // 再次调用同日期：删旧行（级联清旧 breakdown）+ 写新行，daily 不累加、breakdown 全替换。
        let new_stat_id = db
            .replace_daily_stats_for_date(
                "2026-01-01", None, 999, 0, 0, 0, 0, 0.0, None, None,
                &["claude".to_string()], None, None, &[],
            )
            .await
            .unwrap();
        let stats2 = db.get_usage_stats("daily", None, None).await.unwrap();
        assert_eq!(stats2.len(), 1, "重复 replace 不应累加 daily 行");
        assert_eq!(stats2[0].input_tokens, 999, "应替换为新值");
        // 新行不带 breakdown → 为空；旧行已级联删除。
        assert!(
            db.get_usage_model_breakdowns(new_stat_id).await.unwrap().is_empty(),
            "新行不带 breakdown 时应为空"
        );
    }

    /// get_usage_model_breakdowns_by_stat_ids：批量取多 stat 的 breakdown 按 id 分组；空入参返回空 map。
    #[tokio::test]
    async fn test_get_usage_model_breakdowns_by_stat_ids_groups_and_empty() {
        let db = fresh_db().await;
        let one = [ModelBreakdownRow {
            model_name: "m".into(),
            input_tokens: 1,
            output_tokens: 1,
            cache_creation_tokens: 0,
            cache_read_tokens: 0,
            extra_total_tokens: 0,
            cost: 0.0,
        }];
        let s1 = db
            .replace_daily_stats_for_date(
                "2026-02-01", None, 1, 1, 0, 0, 0, 0.0, None, None, &["m".to_string()], None, None, &one,
            )
            .await
            .unwrap();
        let s2 = db
            .replace_daily_stats_for_date(
                "2026-02-02", None, 1, 1, 0, 0, 0, 0.0, None, None, &["m".to_string()], None, None, &one,
            )
            .await
            .unwrap();
        let map = db.get_usage_model_breakdowns_by_stat_ids(&[s1, s2]).await.unwrap();
        assert_eq!(map.len(), 2, "两个 stat 各自带 1 条 breakdown");
        assert!(map.values().all(|v| v.len() == 1));
        assert!(
            db.get_usage_model_breakdowns_by_stat_ids(&[]).await.unwrap().is_empty(),
            "空入参应返回空 map"
        );
    }
}
