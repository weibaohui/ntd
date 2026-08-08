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
    ///
    /// `description` 随首次 INSERT 一并写入（而非建后再 UPDATE）：委派创建之后还要发讨论首帖
    /// 触发执行，若 description 单独 update，中间任一步失败会留下「空描述任务」（CodeRabbit #1）。
    // 8 个参数均为建任务所需的独立字段，函数体是纯字段映射（→ ActiveModel），强行收敛成 struct
    // 反而割裂调用点与 DB 列的一一对应，故豁免 too_many_arguments（线性数据构建，符合豁免场景 #1）。
    #[allow(clippy::too_many_arguments)]
    pub async fn create_delegate_task(
        &self,
        title: &str,
        description: &str,
        workspace_id: i64,
        assignee_kind: &str,
        assignee_name: &str,
        auto_continue: bool,
        // 接力轮数上限覆盖:None=沿用工作空间默认 → 兜底常量(三级解析见 resolve_delegate_max_rounds)。
        delegate_max_rounds: Option<i64>,
    ) -> Result<tasks::Model, sea_orm::DbErr> {
        let now = utc_timestamp();
        let am = tasks::ActiveModel {
            title: ActiveValue::Set(title.to_string()),
            workspace_id: ActiveValue::Set(Some(workspace_id)),
            // 委派任务不绑工艺模板/环路；template_id 沿用 0（与环路默认口径一致），loop_id 留空。
            template_id: ActiveValue::Set(Some(0)),
            loop_id: ActiveValue::Set(None),
            // 需求原文随建写入（见函数注释：避免建后再 update 的原子性缺口）。
            description: ActiveValue::Set(description.to_string()),
            created_at: ActiveValue::Set(Some(now.clone())),
            updated_at: ActiveValue::Set(Some(now)),
            execution_mode: ActiveValue::Set("delegate".to_string()),
            assignee_kind: ActiveValue::Set(Some(assignee_kind.to_string())),
            assignee_name: ActiveValue::Set(Some(assignee_name.to_string())),
            // 布尔开关落库为 0/1 整数（与 SQLite 列类型对齐），handler 校验已保证仅专家可为 true。
            auto_continue: ActiveValue::Set(if auto_continue { 1 } else { 0 }),
            // 接力计数从 0 起，由 completion 接力分支递增（P2）。
            continue_rounds: ActiveValue::Set(0),
            // 任务级上限覆盖随建写入；None 表示沿用工作空间默认（三级可配，见 resolve_delegate_max_rounds）。
            delegate_max_rounds: ActiveValue::Set(delegate_max_rounds),
            ..Default::default()
        };
        am.insert(&self.conn).await
    }

    /// 更新单个委派任务的「接力轮数上限」覆盖（需求 092 护栏配置化）。
    ///
    /// - `Some(n)`（n≥1，由 handler 校验 1..=50）→ 置为 n，覆盖工作空间默认。
    /// - `None` → 置 NULL，回退工作空间默认 → 兜底常量（即「恢复默认」）。
    ///
    /// 任务不存在时静默返回（与 update_task_status 同口径，调用方已先校验存在性）。
    pub async fn update_delegate_max_rounds(
        &self, id: i64, max: Option<i64>,
    ) -> Result<Option<tasks::Model>, sea_orm::DbErr> {
        let existing = tasks::Entity::find_by_id(id).one(&self.conn).await?;
        // 返回更新后的 Model：调用方据此直接 resolve 有效值，免去一次冗余 get_task 重读。
        // （find 已取到行，update 再回写最新态——无需调用方二次查库。）
        if let Some(c) = existing {
            let mut am: tasks::ActiveModel = c.into();
            am.delegate_max_rounds = ActiveValue::Set(max);
            am.updated_at = ActiveValue::Set(Some(utc_timestamp()));
            let updated = am.update(&self.conn).await?;
            return Ok(Some(updated));
        }
        Ok(None)
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

    /// 自动接力轮数 +1 并返回递增后的新值（需求 092 P2 护栏用）。
    ///
    /// 采用 read-modify-write 而非 SQL 自增表达式：接力是顺序事件驱动的（上一轮完成
    /// 回调才触发下一轮），同一任务的 continue_rounds 不存在并发递增，故无需原子自增；
    /// 同时与本文件 update_task_status / update_task_description 的写法保持一致。
    /// 任务不存在时返回 0（调用方据此跳过接力）。
    pub async fn increment_continue_rounds(&self, id: i64) -> Result<i64, sea_orm::DbErr> {
        let existing = tasks::Entity::find_by_id(id).one(&self.conn).await?;
        let Some(task) = existing else {
            return Ok(0);
        };
        let new_rounds = task.continue_rounds + 1;
        let mut am: tasks::ActiveModel = task.into();
        am.continue_rounds = ActiveValue::Set(new_rounds);
        am.updated_at = ActiveValue::Set(Some(utc_timestamp()));
        am.update(&self.conn).await?;
        Ok(new_rounds)
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    /// 全新内存库（跑完所有迁移 + 种子）。
    async fn fresh_db() -> Database {
        Database::new(":memory:").await.expect("memory db must open")
    }

    /// increment_continue_rounds：从 0（委派任务初始）递增到 1、2，
    /// 校验 read-modify-write 正确落库且每次返回递增后的新值。
    #[tokio::test]
    async fn test_increment_continue_rounds_increments_and_persists() {
        let db = fresh_db().await;
        // 委派任务建表即写 continue_rounds=0，是接力的真实载体，用它验证最贴近生产路径。
        let task = db
            .create_delegate_task("接力任务T", "接力需求原文", 1, "expert", "专家A", true, None)
            .await
            .expect("create delegate task");
        assert_eq!(task.continue_rounds, 0, "新建委派任务初始计数应为 0");

        // 第一轮 +1 → 1，并实际落库（get 回读校验，排除「只返回新值但没写库」的假阳性）。
        let r1 = db.increment_continue_rounds(task.id).await.expect("increment #1");
        assert_eq!(r1, 1);
        assert_eq!(
            db.get_task(task.id).await.expect("get").expect("task").continue_rounds,
            1,
            "递增后 DB 中应为 1"
        );

        // 第二轮 +1 → 2，验证不是幂等的「总是返回固定值」。
        let r2 = db.increment_continue_rounds(task.id).await.expect("increment #2");
        assert_eq!(r2, 2);
    }

    /// 任务不存在边界：返回 Ok(0)，调用方据此跳过接力（不报错、不 panic）。
    /// 全新内存库无任何任务，999_999 必然缺失。
    #[tokio::test]
    async fn test_increment_continue_rounds_missing_task_returns_zero() {
        let db = fresh_db().await;
        let r = db
            .increment_continue_rounds(999_999)
            .await
            .expect("missing task should not error");
        assert_eq!(r, 0, "任务不存在时返回 0，调用方据此跳过接力");
    }

    /// update_delegate_max_rounds：置值后再清空，回读校验落库正确。
    #[tokio::test]
    async fn test_update_delegate_max_rounds_set_then_clear() {
        let db = fresh_db().await;
        let task = db
            .create_delegate_task("T", "D", 1, "expert", "专家A", true, None)
            .await
            .expect("create");
        // 初始 None（沿用工作空间默认）。
        assert_eq!(
            db.get_task(task.id).await.unwrap().unwrap().delegate_max_rounds,
            None
        );
        // 置值覆盖。
        db.update_delegate_max_rounds(task.id, Some(8)).await.expect("set");
        assert_eq!(
            db.get_task(task.id).await.unwrap().unwrap().delegate_max_rounds,
            Some(8)
        );
        // None 清空（恢复默认）。
        db.update_delegate_max_rounds(task.id, None).await.expect("clear");
        assert_eq!(
            db.get_task(task.id).await.unwrap().unwrap().delegate_max_rounds,
            None
        );
    }
}
