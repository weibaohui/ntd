//! V74 迁移：数据回填 -- 把 `loop_steps.review_type='ai'` 关联的 todo 的
//! `auto_review_enabled` 置 1，修复「工艺选了 AI 评审却从不触发打分」的存量数据。
//!
//! 背景：installer 此前未把 `link.review_type` 翻译到 `todo.auto_review_enabled`
//! （`create_todo_with_extras` 硬编码 false），导致历史环节 todo 永远不触发自动评审。
//! installer 已补穿透（见 `services/process/installer.rs::create_todo_for_link`），
//! 本迁移负责回填历史数据。幂等：UPDATE 重复执行结果一致。
use sea_orm::{ConnectionTrait, Statement};

use crate::db::migration::Migration;
use crate::db::Database;

pub(super) struct V74BackfillAutoReview;

#[async_trait::async_trait]
impl Migration for V74BackfillAutoReview {
    fn version(&self) -> i64 {
        74
    }
    fn name(&self) -> &'static str {
        "V74BackfillAutoReview"
    }
    async fn up(&self, db: &Database) -> Result<(), sea_orm::DbErr> {
        // 只回填 normal 类型 todo（todo_type=0）；评审实例（todo_type=2）自身保持 false，
        // 避免「评审 todo 完成后又派生评审」的无限递归。
        // 子查询锁定 review_type='ai' 的环节关联 todo，与 installer 穿透语义一致。
        db.conn
            .execute(Statement::from_string(
                sea_orm::DbBackend::Sqlite,
                "UPDATE todos SET auto_review_enabled = 1 \
                 WHERE todo_type = 0 AND (auto_review_enabled = 0 OR auto_review_enabled IS NULL) \
                 AND id IN (SELECT todo_id FROM loop_steps WHERE review_type = 'ai')",
            ))
            .await?;
        Ok(())
    }
}
