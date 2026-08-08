//! 任务讨论帖数据访问（需求 060：任务讨论区）。
//!
//! 所有方法都是 `impl Database` 的薄封装，只做数据读写，不含 @ 解析与执行触发
//! （那部分在 `handlers/task_posts.rs`）。智能体帖的「占位 → 回写结论」状态机
//! 由 `create_task_post`（写 running 占位）+ `finalize_discussion_post`（执行落定时回写）配合完成。

use sea_orm::{ActiveModelTrait, ActiveValue, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder, QuerySelect};

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

    /// 主楼层分页查询（id ASC，parent_post_id IS NULL）。`page` 从 1 起。
    /// 用于讨论 Tab「主楼层分页 + 楼中楼跟随」：按主楼层翻页，避免一次拉全量。
    pub async fn list_main_posts_paged(
        &self,
        task_id: i64,
        page: u64,
        limit: u64,
    ) -> Result<Vec<task_posts::Model>, sea_orm::DbErr> {
        // page 从 1 起：第 1 页 offset=0。saturating 运算防 page 过大时溢出。
        let safe_limit = limit.max(1);
        let offset = page.saturating_sub(1).saturating_mul(safe_limit);
        task_posts::Entity::find()
            .filter(task_posts::Column::TaskId.eq(task_id))
            .filter(task_posts::Column::ParentPostId.is_null())
            .order_by_asc(task_posts::Column::Id)
            .offset(offset)
            .limit(safe_limit)
            .all(&self.conn)
            .await
    }

    /// 最近 N 条主楼层讨论帖，按时间正序（id 升序）返回。
    ///
    /// 与 `list_main_posts_paged`（id ASC 翻页、供讨论 Tab 分页）的区别：本方法服务于
    /// carrier prompt 注入——被 @ 的 AI 需要的是「最近的」讨论上下文，而非最早的。
    /// 实现：DESC 取 N 条（最新），再原地反转为 ASC（时间正序），既是最新的又按时间正序。
    pub async fn list_recent_main_posts(
        &self,
        task_id: i64,
        limit: u64,
    ) -> Result<Vec<task_posts::Model>, sea_orm::DbErr> {
        let safe_limit = limit.max(1);
        let mut posts: Vec<task_posts::Model> = task_posts::Entity::find()
            .filter(task_posts::Column::TaskId.eq(task_id))
            .filter(task_posts::Column::ParentPostId.is_null())
            .order_by_desc(task_posts::Column::Id)
            .limit(safe_limit)
            .all(&self.conn)
            .await?;
        // DESC 取回的是「最新→最旧」，反转为「最旧→最新」（时间正序），与 prompt 标题一致。
        posts.reverse();
        Ok(posts)
    }

    /// 主楼层总数（分页页码计算用；只数 parent_post_id IS NULL 的行）。
    pub async fn count_main_posts(&self, task_id: i64) -> Result<u64, sea_orm::DbErr> {
        task_posts::Entity::find()
            .filter(task_posts::Column::TaskId.eq(task_id))
            .filter(task_posts::Column::ParentPostId.is_null())
            .count(&self.conn)
            .await
    }

    /// 任务级 running 帖总数（不限当前分页）：用于「讨论」Tab 角标，跨页也准（CodeRabbit）。
    /// running 帖恒为 agent 主楼层（status=running），按 status 过滤即可。
    pub async fn count_running_posts(&self, task_id: i64) -> Result<u64, sea_orm::DbErr> {
        task_posts::Entity::find()
            .filter(task_posts::Column::TaskId.eq(task_id))
            .filter(task_posts::Column::Status.eq(STATUS_RUNNING))
            .count(&self.conn)
            .await
    }

    /// 批量取一组主楼层的楼中楼回复（parent_post_id IN parent_ids，id ASC）。
    /// 一次 IN 查询规避逐楼层查询的 N+1；空 parent_ids 直接返回空 Vec，不发 SQL。
    pub async fn list_replies_for(
        &self,
        task_id: i64,
        parent_ids: &[i64],
    ) -> Result<Vec<task_posts::Model>, sea_orm::DbErr> {
        // 空 IN 在某些后端生成非法 SQL，这里短路返回空。
        if parent_ids.is_empty() {
            return Ok(Vec::new());
        }
        task_posts::Entity::find()
            .filter(task_posts::Column::TaskId.eq(task_id))
            .filter(task_posts::Column::ParentPostId.is_in(parent_ids.iter().copied()))
            .order_by_asc(task_posts::Column::Id)
            .all(&self.conn)
            .await
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

    /// 按 source_execution_id 取对应的智能体占位帖（需求 092 P2 自动接力用）。
    ///
    /// 一条讨论类执行只对应一条占位帖（trigger=discussion/discussion_auto）。completion
    /// 回写后用此查询取回帖子的 task_id / expert_name / executor，供接力判断「本次执行者
    /// 是否就是 assignee 管家」与定位所属委派任务。找不到（帖已删）返回 None，调用方跳过接力。
    pub async fn get_task_post_by_source_execution(
        &self,
        record_id: i64,
    ) -> Result<Option<task_posts::Model>, sea_orm::DbErr> {
        task_posts::Entity::find()
            .filter(task_posts::Column::SourceExecutionId.eq(record_id))
            .one(&self.conn)
            .await
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

    /// 造一个主楼层人帖（parent_post_id=None），返回其 id。
    async fn seed_main_post(db: &Database, task_id: i64, content: &str) -> i64 {
        db.create_task_post(NewPost {
            task_id,
            parent_post_id: None,
            kind: KIND_HUMAN,
            author_name: "我",
            executor: None,
            expert_name: None,
            content,
            mentions_json: "[]",
            status: "sent",
            source_execution_id: None,
            source_todo_id: None,
        })
        .await
        .expect("seed main post")
        .id
    }

    /// 造一条楼中楼回复，挂到 `parent_id` 指定的主楼层下（深度 1）。
    async fn seed_reply(db: &Database, task_id: i64, parent_id: i64, content: &str) -> i64 {
        db.create_task_post(NewPost {
            task_id,
            parent_post_id: Some(parent_id),
            kind: KIND_HUMAN,
            author_name: "我",
            executor: None,
            expert_name: None,
            content,
            mentions_json: "[]",
            status: "sent",
            source_execution_id: None,
            source_todo_id: None,
        })
        .await
        .expect("seed reply")
        .id
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

        // 主楼层分页第 1 页命中（刚建的是主楼层）；总数同步为 1。
        let list = db.list_main_posts_paged(task_id, 1, 50).await.expect("list main");
        assert_eq!(list.len(), 1);
        assert_eq!(db.count_main_posts(task_id).await.expect("count"), 1);

        // delete 后再查为空
        let affected = db.delete_task_post(post.id).await.expect("delete");
        assert_eq!(affected, 1);
        assert!(db.get_task_post(post.id).await.expect("get").is_none());
    }

    /// 主楼层分页 + 楼中楼批量查询：造 3 主楼层、其中两个各挂一条楼中楼，
    /// 验证 page/offset 截断、count_total、IN 批量取与空入参短路。
    #[tokio::test]
    async fn test_main_posts_paged_and_replies_for() {
        let db = fresh_db().await;
        let task_id = seed_task(&db).await;
        let m1 = seed_main_post(&db, task_id, "m1").await;
        let m2 = seed_main_post(&db, task_id, "m2").await;
        let m3 = seed_main_post(&db, task_id, "m3").await;
        seed_reply(&db, task_id, m1, "r1").await;
        seed_reply(&db, task_id, m2, "r2").await;

        // 总数 = 3 个主楼层（楼中楼不计入主楼层分页总数）。
        assert_eq!(db.count_main_posts(task_id).await.expect("count"), 3);
        // page=1 limit=2 → 前 2 个主楼层（id ASC）。
        let p1 = db.list_main_posts_paged(task_id, 1, 2).await.expect("page1");
        assert_eq!(p1.len(), 2);
        assert_eq!(p1[0].id, m1, "第 1 页应从最早的主楼层开始");
        // page=2 limit=2 → 第 3 个。
        let p2 = db.list_main_posts_paged(task_id, 2, 2).await.expect("page2");
        assert_eq!(p2.len(), 1);
        assert_eq!(p2[0].id, m3);
        // page=3 → 越界空页。
        assert!(db.list_main_posts_paged(task_id, 3, 2).await.expect("page3").is_empty());

        // IN 批量取 m1/m2 的楼中楼，应得 2 条；m3 无楼中楼不计入。
        let replies = db.list_replies_for(task_id, &[m1, m2, m3]).await.expect("replies");
        assert_eq!(replies.len(), 2, "m1/m2 各一条，m3 无");
        // 空 parent_ids 不发 SQL，直接返回空。
        assert!(db.list_replies_for(task_id, &[]).await.expect("empty in").is_empty());
    }

    /// list_recent_main_posts：返回最新 N 条且按时间正序（id 升序），楼中楼不计入。
    /// 与 list_main_posts_paged（最早 N 条）形成对照，保证 carrier prompt 注入「最近的」上下文。
    #[tokio::test]
    async fn test_list_recent_main_posts_returns_latest_in_asc() {
        let db = fresh_db().await;
        let task_id = seed_task(&db).await;
        let m1 = seed_main_post(&db, task_id, "m1").await;
        let m2 = seed_main_post(&db, task_id, "m2").await;
        let m3 = seed_main_post(&db, task_id, "m3").await;
        let m4 = seed_main_post(&db, task_id, "m4").await;

        // 取最近 2 条：应为 m3、m4（最新两条），且按时间正序（m3 在前）。
        let recent = db.list_recent_main_posts(task_id, 2).await.expect("recent");
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].id, m3, "最新 N 条应按时间正序（较早的在前）");
        assert_eq!(recent[1].id, m4);
        // 较早的 m1/m2 不在最近 2 条内（反向印证取的是「最新」而非「最早」）。
        assert!(recent.iter().all(|p| p.id != m1 && p.id != m2));

        // limit 超过总数：返回全部，仍按时间正序。
        let all = db.list_recent_main_posts(task_id, 100).await.expect("all");
        assert_eq!(all.len(), 4);
        assert_eq!(all[0].id, m1);
        assert_eq!(all[3].id, m4);

        // 楼中楼不计入主楼层。
        seed_reply(&db, task_id, m1, "r1").await;
        let still = db.list_recent_main_posts(task_id, 100).await.expect("still");
        assert_eq!(still.len(), 4, "楼中楼不应计入主楼层");
    }

    /// count_running_posts：只数 status=running 的帖（sent/success/failed 不计），供讨论 Tab 角标。
    #[tokio::test]
    async fn test_count_running_posts_only_counts_running() {
        let db = fresh_db().await;
        let task_id = seed_task(&db).await;
        // 1 条 sent 人帖（不计入）。
        seed_main_post(&db, task_id, "人帖").await;
        // 2 条 running 智能体占位帖（计入）。
        for exec in ["codex", "claude"] {
            db.create_task_post(NewPost {
                task_id,
                parent_post_id: None,
                kind: KIND_AGENT,
                author_name: exec,
                executor: Some(exec),
                expert_name: None,
                content: "正在干活…",
                mentions_json: "[]",
                status: STATUS_RUNNING,
                source_execution_id: Some(1),
                source_todo_id: None,
            })
            .await
            .expect("seed running post");
        }
        assert_eq!(db.count_running_posts(task_id).await.expect("count"), 2);
        // 跨任务隔离：另一任务的 running 帖不计入本 task。
        let other_task = seed_task(&db).await;
        db.create_task_post(NewPost {
            task_id: other_task,
            parent_post_id: None,
            kind: KIND_AGENT,
            author_name: "codex",
            executor: Some("codex"),
            expert_name: None,
            content: "其它任务的 running",
            mentions_json: "[]",
            status: STATUS_RUNNING,
            source_execution_id: Some(2),
            source_todo_id: None,
        })
        .await
        .expect("seed other task running");
        assert_eq!(
            db.count_running_posts(task_id).await.expect("count again"),
            2,
            "另一任务的 running 帖不应计入本任务"
        );
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

    /// soft_delete_todo：置 deleted_at；不存在的 id 静默 no-op（不报错）。
    /// 直接覆盖（finalize_discussion_post 间接路径之外）「标记删除」与「记录缺失」两条分支。
    #[tokio::test]
    async fn test_soft_delete_todo_marks_and_noops_on_missing() {
        let db = fresh_db().await;
        // 用讨论载体 todo 造一条记录（它落在 todos 表，可被软删）。
        let tid = db
            .create_discussion_todo("c".to_string(), "p".to_string(), None, None, 1, "/tmp")
            .await
            .expect("create todo");
        // 软删前 deleted_at 应为空。
        let row_before = db
            .conn
            .query_one(sea_orm::Statement::from_string(
                sea_orm::DbBackend::Sqlite,
                format!("SELECT deleted_at FROM todos WHERE id = {tid}"),
            ))
            .await
            .expect("query before")
            .expect("row exists");
        let before: Option<String> = row_before
            .try_get_by::<Option<String>, _>("deleted_at")
            .ok()
            .flatten();
        assert!(before.is_none(), "软删前 deleted_at 应为空");

        // 软删后 deleted_at 应非空。
        db.soft_delete_todo(tid).await.expect("soft delete");
        let row_after = db
            .conn
            .query_one(sea_orm::Statement::from_string(
                sea_orm::DbBackend::Sqlite,
                format!("SELECT deleted_at FROM todos WHERE id = {tid}"),
            ))
            .await
            .expect("query after")
            .expect("row exists");
        let after: Option<String> = row_after
            .try_get_by::<Option<String>, _>("deleted_at")
            .ok()
            .flatten();
        assert!(after.is_some(), "软删后 deleted_at 应非空");

        // 不存在的 id：静默 no-op，不报错（兜底分支）。
        db.soft_delete_todo(999999).await.expect("noop on missing");
    }

    /// get_task_post_by_source_execution：按执行记录 id 反查占位帖。
    /// 覆盖命中 / 未命中 / 与 source_execution_id=None 的人帖互不干扰（SQL NULL 不参与相等匹配，
    /// 这是接力按 record 精确定位占位帖的前提）。
    #[tokio::test]
    async fn test_get_task_post_by_source_execution() {
        let db = fresh_db().await;
        let task_id = seed_task(&db).await;

        // 占位帖带 source_execution_id=Some(42)（模拟 @ 触发 / 接力回写的 running 帖）。
        let post = db
            .create_task_post(NewPost {
                task_id,
                parent_post_id: None,
                kind: KIND_AGENT,
                author_name: "执行器A",
                executor: Some("执行器A"),
                expert_name: None,
                content: "执行器A 正在干活…",
                mentions_json: "[]",
                status: STATUS_RUNNING,
                source_execution_id: Some(42),
                source_todo_id: None,
            })
            .await
            .expect("create agent post");

        // 命中：按 42 反查到刚建的占位帖。
        let got = db
            .get_task_post_by_source_execution(42)
            .await
            .expect("query")
            .expect("post exists");
        assert_eq!(got.id, post.id);
        assert_eq!(got.source_execution_id, Some(42));

        // 未命中：不存在的执行记录返回 None（帖已删 / 从未关联）。
        assert!(db.get_task_post_by_source_execution(999).await.expect("query none").is_none());

        // 区分：再建一条 source_execution_id=None 的普通人帖，按 42 仍只命中占位帖。
        let _human = seed_main_post(&db, task_id, "普通人帖").await;
        let only42 = db
            .get_task_post_by_source_execution(42)
            .await
            .expect("query 42 again")
            .expect("仍只命中占位帖");
        assert_eq!(only42.id, post.id, "None 帖不应被 NULL 比较误中");
    }
}

