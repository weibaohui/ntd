use sea_orm::{ActiveModelTrait, ActiveValue, EntityTrait, QueryFilter, QueryOrder, ColumnTrait};
use crate::db::entity::tasks;
use crate::db::Database;
use crate::models::utc_timestamp;

impl Database {
    pub async fn create_task(
        &self, title: &str, workspace_id: i64, template_id: i64, loop_id: Option<i64>,
    ) -> Result<tasks::Model, sea_orm::DbErr> {
        let now = utc_timestamp();
        let am = tasks::ActiveModel {
            title: ActiveValue::Set(title.to_string()),
            workspace_id: ActiveValue::Set(Some(workspace_id)),
            template_id: ActiveValue::Set(Some(template_id)),
            loop_id: ActiveValue::Set(loop_id),
            created_at: ActiveValue::Set(Some(now.clone())),
            updated_at: ActiveValue::Set(Some(now)),
            ..Default::default()
        };
        am.insert(&self.conn).await
    }

    /// 创建「委派」任务（execution_mode='delegate'）：不绑工艺环路，转而指定一个处理人
    /// （专家或执行器）执行，可选开启自动接力（仅专家允许）。
    ///
    /// 与 [`create_task`](Self::create_task)（环路模式）拆成两个方法而非扩参，是因为
    /// 环路模式已被 handlers/tests 多处调用，给它塞一排 Option 委派参数会污染所有调用点、
    /// 还要每处判空；委派路径字段语义独立，单列方法更清晰，也符合「小函数单一职责」。
    /// 两者最终都落到 tasks::ActiveModel，仅写入字段不同。
    pub async fn create_delegate_task(
        &self,
        title: &str,
        workspace_id: i64,
        assignee_kind: &str,
        assignee_name: &str,
        auto_continue: bool,
    ) -> Result<tasks::Model, sea_orm::DbErr> {
        let now = utc_timestamp();
        let am = tasks::ActiveModel {
            title: ActiveValue::Set(title.to_string()),
            workspace_id: ActiveValue::Set(Some(workspace_id)),
            // 委派任务不绑工艺模板/环路；template_id 沿用 0（与环路默认口径一致），loop_id 留空。
            template_id: ActiveValue::Set(Some(0)),
            loop_id: ActiveValue::Set(None),
            created_at: ActiveValue::Set(Some(now.clone())),
            updated_at: ActiveValue::Set(Some(now)),
            execution_mode: ActiveValue::Set("delegate".to_string()),
            assignee_kind: ActiveValue::Set(Some(assignee_kind.to_string())),
            assignee_name: ActiveValue::Set(Some(assignee_name.to_string())),
            // 布尔开关落库为 0/1 整数（与 SQLite 列类型对齐），handler 校验已保证仅专家可为 true。
            auto_continue: ActiveValue::Set(if auto_continue { 1 } else { 0 }),
            // 接力计数从 0 起，由 completion 接力分支递增（P2）。
            continue_rounds: ActiveValue::Set(0),
            ..Default::default()
        };
        am.insert(&self.conn).await
    }

    pub async fn get_task(&self, id: i64) -> Result<Option<tasks::Model>, sea_orm::DbErr> {
        tasks::Entity::find_by_id(id).one(&self.conn).await
    }

    /// 列出任务：按 workspace_id 过滤，可选按 status 过滤。
    /// workspace_id 是必填项，避免不同工作空间任务串台（修复 list_tasks 忽略 ws 的 bug）。
    pub async fn list_tasks(
        &self, workspace_id: i64, status: Option<&str>,
    ) -> Result<Vec<tasks::Model>, sea_orm::DbErr> {
        let mut q = tasks::Entity::find().filter(tasks::Column::WorkspaceId.eq(workspace_id));
        if let Some(s) = status { q = q.filter(tasks::Column::Status.eq(s)); }
        // 054 起统一按 id DESC（列表默认排序），替代原 created_at DESC 口径。
        q.order_by_desc(tasks::Column::Id).all(&self.conn).await
    }

    pub async fn update_task_status(&self, id: i64, status: &str) -> Result<(), sea_orm::DbErr> {
        let existing = tasks::Entity::find_by_id(id).one(&self.conn).await?;
        if let Some(c) = existing {
            let mut am: tasks::ActiveModel = c.into();
            am.status = ActiveValue::Set(status.to_string());
            am.updated_at = ActiveValue::Set(Some(utc_timestamp()));
            am.update(&self.conn).await?;
        }
        Ok(())
    }

    pub async fn update_task_description(&self, id: i64, desc: &str) -> Result<(), sea_orm::DbErr> {
        let existing = tasks::Entity::find_by_id(id).one(&self.conn).await?;
        if let Some(c) = existing {
            let mut am: tasks::ActiveModel = c.into();
            am.description = ActiveValue::Set(desc.to_string());
            am.updated_at = ActiveValue::Set(Some(utc_timestamp()));
            am.update(&self.conn).await?;
        }
        Ok(())
    }

    /// 硬删除单个任务。
    pub async fn delete_task(&self, id: i64) -> Result<(), sea_orm::DbErr> {
        tasks::Entity::delete_by_id(id).exec(&self.conn).await?;
        Ok(())
    }

    /// 批量硬删除任务（返回成功删除数）。
    pub async fn batch_delete_tasks(&self, ids: &[i64]) -> Result<u64, sea_orm::DbErr> {
        if ids.is_empty() { return Ok(0); }
        let res = tasks::Entity::delete_many()
            .filter(tasks::Column::Id.is_in(ids.to_vec()))
            .exec(&self.conn)
            .await?;
        Ok(res.rows_affected)
    }
}
