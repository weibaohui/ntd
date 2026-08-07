//! 数据库迁移 V90：为高频读路径补建性能索引。
//!
//! ## 背景
//! 性能扫描（091 性能优化）发现多个高频查询过滤/排序的列没有索引，
//! 随数据量增长退化为全表扫描，拖慢列表/详情/聚合等读路径。本迁移一次性补齐。
//! 索引清单与设计文档 `docs/design/091-性能优化-设计.md` §2.C1 对应。
//!
//! ## 幂等
//! 全部用 `CREATE INDEX IF NOT EXISTS`，已存在则静默跳过，可安全重复执行。

use async_trait::async_trait;

use super::super::Database;
use super::Migration;

pub(super) struct V90AddPerformanceIndexes;

#[async_trait]
impl Migration for V90AddPerformanceIndexes {
    fn version(&self) -> i64 {
        90
    }

    fn name(&self) -> &'static str {
        "add_performance_indexes"
    }

    /// 批量创建性能索引。每条索引服务一个被全表扫描拖慢的高频读路径。
    async fn up(&self, db: &Database) -> Result<(), sea_orm::DbErr> {
        // 每行行尾注释说明该索引服务的查询路径（即「为何需要它」），便于后续评估是否仍必要。
        const INDEXES: &[&str] = &[
            // 事项列表 / 调度器按 workspace 过滤 todos 的主路径，是增长最快的表。
            "CREATE INDEX IF NOT EXISTS idx_todos_workspace_id ON todos(workspace_id)",
            // loop 执行详情按 execution 取 step 列表（该表此前 0 索引，每次全扫）。
            "CREATE INDEX IF NOT EXISTS idx_loop_step_executions_loop_exec \
             ON loop_step_executions(loop_execution_id, sequence_index)",
            // 飞书历史消息按 bot+chat 拉取并按时间倒序，覆盖增量拉取器与历史页。
            "CREATE INDEX IF NOT EXISTS idx_feishu_messages_bot_chat \
             ON feishu_messages(bot_id, chat_id, created_at DESC)",
            // 每次执行完成回写讨论帖时按 source_execution_id 定位帖子。
            "CREATE INDEX IF NOT EXISTS idx_task_posts_source_execution_id \
             ON task_posts(source_execution_id)",
            // 讨论区执行明细按 workspace 直查 record（V89 加列但未建索引）。
            "CREATE INDEX IF NOT EXISTS idx_execution_records_workspace_id \
             ON execution_records(workspace_id)",
            // 环路按 workspace 聚合 token 汇总 / 列表过滤。
            "CREATE INDEX IF NOT EXISTS idx_loops_workspace_id ON loops(workspace_id)",
            // 环路列表展示工艺模板名称关联。
            "CREATE INDEX IF NOT EXISTS idx_loops_process_template_id \
             ON loops(process_template_id)",
            // 任务列表取每个 task 的最近一次执行状态（消除 N+1 的前置索引）。
            "CREATE INDEX IF NOT EXISTS idx_loop_executions_task_id \
             ON loop_executions(task_id)",
            // 按 todo 反查其所属环路环节（引用计数 / 删除前校验）。
            "CREATE INDEX IF NOT EXISTS idx_loop_steps_todo_id ON loop_steps(todo_id)",
            // 讨论区主帖 / 回复分页：按 task + 父帖定位楼层。
            "CREATE INDEX IF NOT EXISTS idx_task_posts_task_parent \
             ON task_posts(task_id, parent_post_id, id)",
            // 仪表盘按时间区间聚合技能调用统计。
            "CREATE INDEX IF NOT EXISTS idx_skill_invocations_invoked_at \
             ON skill_invocations(invoked_at)",
            // 任务列表 / 详情按 workspace 过滤（tasks 表此前 0 索引）。
            "CREATE INDEX IF NOT EXISTS idx_tasks_workspace_id ON tasks(workspace_id)",
        ];
        // 逐条执行：IF NOT EXISTS 保证幂等，重复跑升级不会报错。
        for sql in INDEXES {
            db.exec(sql).await?;
        }
        tracing::info!("V90: 已创建 {} 个性能索引", INDEXES.len());
        Ok(())
    }
}

#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use sea_orm::ConnectionTrait;

    /// 抽取「确认某个索引是否存在」的查询，避免在多个用例里重复样板。
    async fn index_exists(db: &Database, name: &str) -> bool {
        let row = db
            .conn
            .query_one(sea_orm::Statement::from_sql_and_values(
                sea_orm::DbBackend::Sqlite,
                "SELECT COUNT(*) AS cnt FROM sqlite_master \
                 WHERE type='index' AND name=?1",
                [name.into()],
            ))
            .await
            .expect("query must succeed")
            .expect("row must exist");
        let cnt: i64 = row.try_get_by("cnt").expect("cnt readable");
        cnt > 0
    }

    /// 验证 V90 创建了代表性索引（覆盖此前 0 索引的 loop_step_executions 与 tasks）。
    #[tokio::test]
    async fn test_v90_creates_representative_indexes() {
        let db = Database::new(":memory:")
            .await
            .expect(":memory: db must open");
        V90AddPerformanceIndexes
            .up(&db)
            .await
            .expect("V90 migration must succeed");

        assert!(
            index_exists(&db, "idx_loop_step_executions_loop_exec").await,
            "loop_step_executions 索引应存在"
        );
        assert!(
            index_exists(&db, "idx_tasks_workspace_id").await,
            "tasks 索引应存在"
        );
        assert!(
            index_exists(&db, "idx_todos_workspace_id").await,
            "todos 索引应存在"
        );
    }

    /// 幂等：重复执行不报错（IF NOT EXISTS）。
    #[tokio::test]
    async fn test_v90_is_idempotent() {
        let db = Database::new(":memory:")
            .await
            .expect(":memory: db must open");
        V90AddPerformanceIndexes.up(&db).await.expect("first run");
        V90AddPerformanceIndexes
            .up(&db)
            .await
            .expect("second run (idempotent)");
    }
}
