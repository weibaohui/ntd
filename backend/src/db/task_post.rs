//! 任务讨论帖数据访问（需求 060：任务讨论区）。
//!
//! 所有方法都是 `impl Database` 的薄封装，只做数据读写，不含 @ 解析与执行触发
//! （那部分在 `handlers/task_posts.rs`）。智能体帖的「占位 → 回写结论」状态机
//! 由 `create_task_post`（写 running 占位）+ `finalize_discussion_post`（执行落定时回写）配合完成。

use sea_orm::{ActiveModelTrait, ActiveValue, ColumnTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect};

use crate::db::entity::task_posts;
use crate::db::Database;
use crate::models::utc_timestamp;

/// 帖子类型常量：与 `kind` 列取值对齐，避免字符串散落。
pub const KIND_HUMAN: &str = "human";
/// 智能体帖（@触发执行后回写）。
pub const KIND_AGENT: &str = "agent";

/// 智能体帖状态：执行中（占位帖）。
pub const STATUS_RUNNING: &str = "running";
/// 智能体帖状态：执行成功，正文已回写结论。
pub const STATUS_SUCCESS: &str = "success";
/// 智能体帖状态：执行失败。
pub const STATUS_FAILED: &str = "failed";

/// 创建帖子的入参：字段较多，用结构体收口，避免函数参数列表过长（可读性）。
pub struct NewPost<'a> {
    pub task_id: i64,
    /// None=主楼层；Some(id)=回复指定楼层（应用层已保证目标为主楼层，深度≤1）。
    pub parent_post_id: Option<i64>,
    pub kind: &'a str,
    pub author_name: &'a str,
    pub executor: Option<&'a str>,
    pub expert_name: Option<&'a str>,
    pub content: &'a str,
    /// 已序列化好的 mentions JSON 字符串。
    pub mentions_json: &'a str,
    pub status: &'a str,
    pub source_execution_id: Option<i64>,
    pub source_todo_id: Option<i64>,
}

impl Database {
    /// 插入一条讨论帖，返回完整 Model（含生成的 id）。
    pub async fn create_task_post(
        &self,
        p: NewPost<'_>,
    ) -> Result<task_posts::Model, sea_orm::DbErr> {
        let now = utc_timestamp();
        let am = task_posts::ActiveModel {
            task_id: ActiveValue::Set(p.task_id),
            parent_post_id: ActiveValue::Set(p.parent_post_id),
            kind: ActiveValue::Set(p.kind.to_string()),
            author_name: ActiveValue::Set(p.author_name.to_string()),
            executor: ActiveValue::Set(p.executor.map(|s| s.to_string())),
            expert_name: ActiveValue::Set(p.expert_name.map(|s| s.to_string())),
            content: ActiveValue::Set(p.content.to_string()),
            mentions: ActiveValue::Set(p.mentions_json.to_string()),
            status: ActiveValue::Set(p.status.to_string()),
            source_execution_id: ActiveValue::Set(p.source_execution_id),
            source_todo_id: ActiveValue::Set(p.source_todo_id),
            created_at: ActiveValue::Set(Some(now.clone())),
            updated_at: ActiveValue::Set(Some(now)),
            ..Default::default()
        };
        am.insert(&self.conn).await
    }

    /// 单帖查询（轮询占位帖状态、跳转详情用）。
    pub async fn get_task_post(
        &self,
        id: i64,
    ) -> Result<Option<task_posts::Model>, sea_orm::DbErr> {
        task_posts::Entity::find_by_id(id).one(&self.conn).await
    }

    /// 列出某任务的全部帖子（主楼层 + 楼中楼），id ASC。
    /// 讨论 Tab 一次拉全量由前端按 parent_post_id 分组，避免 N+1。
    pub async fn list_all_task_posts(
        &self,
        task_id: i64,
        limit: u64,
    ) -> Result<Vec<task_posts::Model>, sea_orm::DbErr> {
        task_posts::Entity::find()
            .filter(task_posts::Column::TaskId.eq(task_id))
            .order_by_asc(task_posts::Column::Id)
            .limit(limit.max(1))
            .all(&self.conn)
            .await
    }

    /// 列出某任务的帖子，按 parent 维度过滤，id ASC（楼层时间顺序）。
    ///
    /// - `parent_id = None` → 主楼层（parent_post_id IS NULL），用 `limit` 截断；
    /// - `parent_id = Some(id)` → 该楼层下的楼中楼回复，调用方传大 limit 取全量。
    pub async fn list_task_posts(
        &self,
        task_id: i64,
        parent_id: Option<i64>,
        limit: u64,
    ) -> Result<Vec<task_posts::Model>, sea_orm::DbErr> {
        let q = task_posts::Entity::find()
            .filter(task_posts::Column::TaskId.eq(task_id))
            // 主楼层走 IS NULL，楼中楼走等值；两个分支只是 parent 条件不同，其余一致。
            .filter(match parent_id {
                None => task_posts::Column::ParentPostId.is_null(),
                Some(pid) => task_posts::Column::ParentPostId.eq(pid),
            })
            .order_by_asc(task_posts::Column::Id);
        // limit(0) 在 SeaORM 语义为「不限制」，这里保证至少取 limit 条。
        q.limit(limit.max(1)).all(&self.conn).await
    }

    /// 执行完成时把结论回写到对应的智能体占位帖（completion 回调用）。
    ///
    /// 定位：`source_execution_id = record_id`（一条执行只对应一条占位帖）。
    /// 找不到（帖子已被删）则静默返回 0，不影响执行本身的成功落定。
    /// `success` 决定 status；`result` 写入正文；`executor` 仅在原帖缺执行器时补。
    pub async fn finalize_discussion_post(
        &self,
        record_id: i64,
        success: bool,
        result: &str,
        executor: Option<&str>,
    ) -> Result<u64, sea_orm::DbErr> {
        let post = task_posts::Entity::find()
            .filter(task_posts::Column::SourceExecutionId.eq(record_id))
            .one(&self.conn)
            .await?;
        let Some(post) = post else { return Ok(0); };
        // 回写前先记下载体 todo id，update 会消费 post（into），之后用不到原值了。
        let carrier_todo_id = post.source_todo_id;
        let status = if success { STATUS_SUCCESS } else { STATUS_FAILED };
        // 执行可能无文本输出（如失败时 stderr 未被当作 result），
        // 用兜底文案避免回写出一条空帖（用户只看到「失败」却不知发生了什么）。
        let final_content = if result.trim().is_empty() {
            if success {
                "（执行完成，未产生文本结论）"
            } else {
                "执行失败（未产生输出），请查看下方执行明细日志。"
            }
        } else {
            result
        };
        let mut am: task_posts::ActiveModel = post.into();
        am.content = ActiveValue::Set(final_content.to_string());
        am.status = ActiveValue::Set(status.to_string());
        // @专家 场景创建时不知道承载执行器，这里用执行记录里的实际执行器补上。
        if let Some(ex) = executor {
            am.executor = ActiveValue::Set(Some(ex.to_string()));
        }
        am.updated_at = ActiveValue::Set(Some(utc_timestamp()));
        am.update(&self.conn).await?;
        // 软删载体 todo：执行已结束，让所有 deleted_at IS NULL 查询兜底排除它，
        // 避免讨论载体 Todo 残留在事项中心 / 列表 / 计数里（执行记录不受影响，仍可跳转）。
        if let Some(tid) = carrier_todo_id {
            let _ = self.soft_delete_todo(tid).await;
        }
        Ok(1)
    }

    /// 硬删一条帖子（删 running 帖由调用方联动取消执行）。
    pub async fn delete_task_post(&self, id: i64) -> Result<u64, sea_orm::DbErr> {
        let res = task_posts::Entity::delete_by_id(id)
            .exec(&self.conn)
            .await?;
        Ok(res.rows_affected)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::db::Database;
    use sea_orm::ConnectionTrait;

    /// 全新内存库（跑完所有迁移 + 种子，含默认执行器）。
    async fn fresh_db() -> Database {
        Database::new(":memory:").await.expect("memory db must open")
    }

    /// 建一个任务供帖子挂载（tasks 表最小必填：title + workspace + template）。
    async fn seed_task(db: &Database) -> i64 {
        db.create_task("讨论任务T", 1, 0, None).await.expect("seed task").id
    }

    /// create → get → list → delete 全链路。
    #[tokio::test]
    async fn test_create_get_list_delete_post() {
        let db = fresh_db().await;
        let task_id = seed_task(&db).await;

        let post = db
            .create_task_post(NewPost {
                task_id,
                parent_post_id: None,
                kind: KIND_HUMAN,
                author_name: "我",
                executor: None,
                expert_name: None,
                content: "你好",
                mentions_json: "[]",
                status: "sent",
                source_execution_id: None,
                source_todo_id: None,
            })
            .await
            .expect("create post");

        // get 命中
        let got = db.get_task_post(post.id).await.expect("get").expect("post exists");
        assert_eq!(got.content, "你好");
        assert_eq!(got.kind, KIND_HUMAN);

        // list 命中（主楼层）
        let list = db.list_all_task_posts(task_id, 50).await.expect("list");
        assert_eq!(list.len(), 1);

        // delete 后再查为空
        let affected = db.delete_task_post(post.id).await.expect("delete");
        assert_eq!(affected, 1);
        assert!(db.get_task_post(post.id).await.expect("get").is_none());
    }

    /// finalize 把 running 占位帖回写为结论，并软删载体 todo。
    #[tokio::test]
    async fn test_finalize_discussion_post_writes_result_and_soft_deletes_carrier() {
        let db = fresh_db().await;
        let task_id = seed_task(&db).await;

        // 建一个隐藏载体 todo 作为 source_todo_id。
        let carrier = db
            .create_discussion_todo(
                "载体".to_string(),
                "prompt".to_string(),
                None,
                Some("前端架构师"),
                1,
                "/tmp",
            )
            .await
            .expect("create carrier todo");

        // 占位帖：running，关联到一条「执行记录」12345（FK 未强制，测试用任意 id）。
        let post = db
            .create_task_post(NewPost {
                task_id,
                parent_post_id: None,
                kind: KIND_AGENT,
                author_name: "前端架构师",
                executor: None,
                expert_name: Some("前端架构师"),
                content: "前端架构师 正在干活…",
                mentions_json: "[]",
                status: STATUS_RUNNING,
                source_execution_id: Some(12345),
                source_todo_id: Some(carrier),
            })
            .await
            .expect("create placeholder");

        // 执行成功 → 回写结论。
        let n = db.finalize_discussion_post(12345, true, "结论：可行", None).await.expect("finalize");
        assert_eq!(n, 1, "应回写一条");
        let updated = db.get_task_post(post.id).await.expect("get").expect("exists");
        assert_eq!(updated.content, "结论：可行");
        assert_eq!(updated.status, STATUS_SUCCESS);

        // 载体 todo 应被软删（deleted_at 非空），从而被事项中心排除。
        let row = db
            .conn
            .query_one(sea_orm::Statement::from_string(
                sea_orm::DbBackend::Sqlite,
                format!("SELECT deleted_at FROM todos WHERE id = {carrier}"),
            ))
            .await
            .expect("query carrier");
        let deleted_at: Option<String> = row
            .and_then(|r| r.try_get_by::<Option<String>, _>("deleted_at").ok())
            .unwrap_or(None);
        assert!(deleted_at.is_some(), "载体 todo 应已软删");

        // 失败分支：再 finalize 一次（同 record）会把状态改成 failed。
        db.finalize_discussion_post(12345, false, "失败原因", None).await.expect("finalize failed");
        let updated2 = db.get_task_post(post.id).await.expect("get").expect("exists");
        assert_eq!(updated2.status, STATUS_FAILED);
    }

    /// finalize 找不到对应执行记录时静默返回 0（帖子已被删的兜底）。
    #[tokio::test]
    async fn test_finalize_discussion_post_missing_returns_zero() {
        let db = fresh_db().await;
        let n = db.finalize_discussion_post(999999, true, "x", None).await.expect("finalize");
        assert_eq!(n, 0);
    }

    /// finalize 在结果为空（成功无结论 / 失败无输出）时用兜底文案，避免回写空帖。
    #[tokio::test]
    async fn test_finalize_discussion_post_empty_result_fallback() {
        let db = fresh_db().await;
        let task_id = seed_task(&db).await;
        let post = db
            .create_task_post(NewPost {
                task_id,
                parent_post_id: None,
                kind: KIND_AGENT,
                author_name: "codex",
                executor: Some("codex"),
                expert_name: None,
                content: "codex 正在干活…",
                mentions_json: "[]",
                status: STATUS_RUNNING,
                source_execution_id: Some(777),
                source_todo_id: None,
            })
            .await
            .expect("create placeholder");

        // 成功但无文本结论 → 兜底文案 + success。
        db.finalize_discussion_post(777, true, "", None).await.expect("finalize");
        let p = db.get_task_post(post.id).await.expect("get").expect("exists");
        assert_eq!(p.status, STATUS_SUCCESS);
        assert!(!p.content.is_empty(), "空结果应有兜底文案");

        // 失败且无输出 → 兜底文案 + failed。
        db.finalize_discussion_post(777, false, "   ", None).await.expect("finalize");
        let p2 = db.get_task_post(post.id).await.expect("get").expect("exists");
        assert_eq!(p2.status, STATUS_FAILED);
        assert!(!p2.content.is_empty(), "空结果应有兜底文案");
    }

    /// create_discussion_todo 应把载体标记为 todo_type=4 并写入 expert_name。
    #[tokio::test]
    async fn test_create_discussion_todo_carrier_fields() {
        let db = fresh_db().await;
        let id = db
            .create_discussion_todo(
                "讨论触发".to_string(),
                "prompt".to_string(),
                None,
                Some("前端架构师"),
                1,
                "/tmp",
            )
            .await
            .expect("create carrier");
        let row = db
            .conn
            .query_one(sea_orm::Statement::from_string(
                sea_orm::DbBackend::Sqlite,
                format!("SELECT todo_type, expert_name FROM todos WHERE id = {id}"),
            ))
            .await
            .expect("query")
            .expect("row exists");
        let todo_type: i64 = row.try_get_by::<i64, _>("todo_type").unwrap_or(0);
        let expert: Option<String> = row.try_get_by::<Option<String>, _>("expert_name").ok().flatten();
        assert_eq!(todo_type, 4, "载体 todo_type 必须为 4（DISCUSSION）");
        assert_eq!(expert.as_deref(), Some("前端架构师"));
    }

    /// 隐藏过滤：讨论载体 todo 不计入工作空间 todo 计数。
    #[tokio::test]
    async fn test_discussion_carrier_excluded_from_workspace_count() {
        let db = fresh_db().await;
        let before = db.count_todos_by_workspace(1).await.expect("count before");
        // 建一个讨论载体 todo（workspace 1）。
        db.create_discussion_todo("c".to_string(), "p".to_string(), None, None, 1, "/tmp")
            .await
            .expect("create carrier");
        let after = db.count_todos_by_workspace(1).await.expect("count after");
        assert_eq!(before, after, "讨论载体 todo 不应计入工作空间 todo 数");
    }
}

