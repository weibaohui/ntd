//! blackboard_pages 数据库层方法。
//!
//! 提供黑板 Wiki 页面的 CRUD 操作，按 (workspace_id, slug) 唯一约束管理。
//! 页面类型分 index / topic / log，各自有不同的更新策略：
//! - topic：由 LLM 产出内容，Service 层调用 upsert_page
//! - index：由后端自动生成，Service 层调用 upsert_page 覆盖
//! - log：由后端追加，调用 append_log_entry

use sea_orm::{
    sea_query::OnConflict, ActiveModelTrait, ColumnTrait, EntityTrait,
    Order, QueryFilter, QueryOrder, Set,
};

use super::entity::blackboard_pages;
use super::Database;

/// 页面类型常量：目录页（后端自动生成）
pub const PAGE_TYPE_INDEX: &str = "index";
/// 页面类型常量：主题页（LLM 产出）
pub const PAGE_TYPE_TOPIC: &str = "topic";
/// 页面类型常量：日志页（后端自动生成）
pub const PAGE_TYPE_LOG: &str = "log";

impl Database {
    /// 获取指定工作空间的所有页面，按 page_type 和 updated_at 排序。
    ///
    /// 排序规则：index 最前 → topic 中间 → log 最后，
    /// 同类型内按 updated_at 倒序，最新更新的在前。
    /// 返回 Vec<Model>，空 workspace 返回空向量。
    pub async fn list_blackboard_pages(
        &self,
        workspace_id: i64,
    ) -> Result<Vec<blackboard_pages::Model>, sea_orm::DbErr> {
        // 用 CASE WHEN 显式控制 page_type 排序值：index=0, topic=1, log=2
        // 避免依赖字母序（index < log < topic）导致 log 排在 topic 前面
        // 同类型页面按 UpdatedAt 降序，最近更新的排在前面
        blackboard_pages::Entity::find()
            .filter(blackboard_pages::Column::WorkspaceId.eq(workspace_id))
            .order_by(
                sea_orm::sea_query::Expr::cust(
                    "CASE page_type WHEN 'index' THEN 0 WHEN 'topic' THEN 1 WHEN 'log' THEN 2 ELSE 3 END"
                ),
                Order::Asc,
            )
            .order_by(blackboard_pages::Column::UpdatedAt, Order::Desc)
            .all(&self.conn)
            .await
    }

    /// 按 slug 获取单个页面。
    ///
    /// 返回 Option<Model>，None 表示该 slug 不存在。
    pub async fn get_blackboard_page(
        &self,
        workspace_id: i64,
        slug: &str,
    ) -> Result<Option<blackboard_pages::Model>, sea_orm::DbErr> {
        blackboard_pages::Entity::find()
            .filter(blackboard_pages::Column::WorkspaceId.eq(workspace_id))
            .filter(blackboard_pages::Column::Slug.eq(slug))
            .one(&self.conn)
            .await
    }

    /// 获取指定工作空间的所有 topic 页面（不含 index/log）。
    ///
    /// 供后端重新生成 index 页面时使用：只需要 topic 页的 slug/title/source_refs。
    pub async fn list_topic_pages(
        &self,
        workspace_id: i64,
    ) -> Result<Vec<blackboard_pages::Model>, sea_orm::DbErr> {
        blackboard_pages::Entity::find()
            .filter(blackboard_pages::Column::WorkspaceId.eq(workspace_id))
            .filter(blackboard_pages::Column::PageType.eq(PAGE_TYPE_TOPIC))
            .order_by(blackboard_pages::Column::UpdatedAt, Order::Desc)
            .all(&self.conn)
            .await
    }

    /// Upsert 页面：记录不存在则创建，存在则按 slug 更新内容和 source_refs。
    ///
    /// 使用 ON CONFLICT(workspace_id, slug) DO UPDATE，
    /// 一次往返完成创建/更新判断 + 写入。
    /// source_refs 采用"合并"策略：追加新 record_ids 到现有列表（去重）。
    pub async fn upsert_blackboard_page(
        &self,
        workspace_id: i64,
        page_type: &str,
        slug: &str,
        title: &str,
        summary: &str,
        content: &str,
        source_refs: &[i64],
    ) -> Result<(), sea_orm::DbErr> {
        // 若页面已存在，先读取现有 source_refs 做合并；不存在则用新值
        let merged_refs = self.merge_source_refs(workspace_id, slug, source_refs).await?;
        let refs_json = serde_json::to_string(&merged_refs).unwrap_or_else(|_| "[]".to_string());
        let now = crate::models::utc_timestamp();

        let am = blackboard_pages::ActiveModel {
            workspace_id: Set(workspace_id),
            page_type: Set(page_type.to_string()),
            slug: Set(slug.to_string()),
            title: Set(title.to_string()),
            summary: Set(summary.to_string()),
            content: Set(content.to_string()),
            source_refs: Set(refs_json),
            updated_at: Set(Some(now.clone())),
            created_at: Set(Some(now)),
            ..Default::default()
        };
        // ON CONFLICT(workspace_id, slug)：命中后覆盖 content/title/source_refs/updated_at
        blackboard_pages::Entity::insert(am)
            .on_conflict(
                OnConflict::columns([
                    blackboard_pages::Column::WorkspaceId,
                    blackboard_pages::Column::Slug,
                ])
                .update_columns([
                    blackboard_pages::Column::PageType,
                    blackboard_pages::Column::Title,
                    blackboard_pages::Column::Summary,
                    blackboard_pages::Column::Content,
                    blackboard_pages::Column::SourceRefs,
                    blackboard_pages::Column::UpdatedAt,
                ])
                .to_owned(),
            )
            .exec(&self.conn)
            .await?;
        Ok(())
    }

    /// 合并 source_refs：读取现有页面的 source_refs，追加新 ids（去重）。
    ///
    /// 页面不存在时直接返回新 ids；存在时合并去重。
    async fn merge_source_refs(
        &self,
        workspace_id: i64,
        slug: &str,
        new_refs: &[i64],
    ) -> Result<Vec<i64>, sea_orm::DbErr> {
        // 读取现有页面；不存在说明是新建，直接用 new_refs
        let existing = self.get_blackboard_page(workspace_id, slug).await?;
        let Some(existing) = existing else {
            return Ok(new_refs.to_vec());
        };
        // 解析现有 refs，合并去重
        let mut refs: Vec<i64> = serde_json::from_str(&existing.source_refs).unwrap_or_default();
        for &id in new_refs {
            if !refs.contains(&id) {
                refs.push(id);
            }
        }
        Ok(refs)
    }

    /// 追加 log 条目：在 log 页面的 content 末尾追加一条记录。
    ///
    /// log 页面是追加式的，每次摄入后追加一条，永不修改旧内容。
    /// 若 log 页面不存在则创建。
    pub async fn append_log_entry(
        &self,
        workspace_id: i64,
        entry_markdown: &str,
    ) -> Result<(), sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        // 读取现有 log 页面
        let existing = self.get_blackboard_page(workspace_id, PAGE_TYPE_LOG).await?;
        match existing {
            Some(model) => {
                // 追加新条目到末尾
                let new_content = format!("{}\n\n{}", model.content, entry_markdown);
                let mut am: blackboard_pages::ActiveModel = model.into();
                am.content = Set(new_content);
                am.updated_at = Set(Some(now));
                am.update(&self.conn).await?;
            }
            None => {
                // 首次创建 log 页面，content 就是第一条条目
                let am = blackboard_pages::ActiveModel {
                    workspace_id: Set(workspace_id),
                    page_type: Set(PAGE_TYPE_LOG.to_string()),
                    slug: Set(PAGE_TYPE_LOG.to_string()),
                    title: Set("更新日志".to_string()),
                    summary: Set("按时间记录的黑板更新日志".to_string()),
                    content: Set(entry_markdown.to_string()),
                    source_refs: Set("[]".to_string()),
                    updated_at: Set(Some(now.clone())),
                    created_at: Set(Some(now)),
                    ..Default::default()
                };
                blackboard_pages::Entity::insert(am).exec(&self.conn).await?;
            }
        }
        Ok(())
    }

    /// 删除指定 slug 的页面。
    ///
    /// 供后期用户编辑/删除页面功能使用。当前阶段主要为测试清理。
    pub async fn delete_blackboard_page(
        &self,
        workspace_id: i64,
        slug: &str,
    ) -> Result<(), sea_orm::DbErr> {
        blackboard_pages::Entity::delete_many()
            .filter(blackboard_pages::Column::WorkspaceId.eq(workspace_id))
            .filter(blackboard_pages::Column::Slug.eq(slug))
            .exec(&self.conn)
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    /// 创建一个测试用工作空间，返回其 id。
    async fn create_test_workspace(db: &Database) -> i64 {
        db.create_project_directory("/tmp/test-blackboard-pages", None, false, false)
            .await
            .expect("create workspace must succeed")
    }

    /// 验证 list_blackboard_pages 在无页面时返回空向量。
    #[tokio::test]
    async fn test_list_blackboard_pages_empty() {
        let db = Database::new(":memory:").await.expect("db must open");
        let ws_id = create_test_workspace(&db).await;
        let pages = db.list_blackboard_pages(ws_id).await.unwrap();
        assert!(pages.is_empty());
    }

    /// 验证 upsert 新建页面后可以按 slug 查到。
    #[tokio::test]
    async fn test_upsert_creates_page() {
        let db = Database::new(":memory:").await.expect("db must open");
        let ws_id = create_test_workspace(&db).await;

        db.upsert_blackboard_page(
            ws_id,
            PAGE_TYPE_TOPIC,
            "auth-module",
            "认证模块",
            "关于认证的结论汇总",
            "# 认证模块\n\n内容",
            &[42, 45],
        )
        .await
        .unwrap();

        let page = db.get_blackboard_page(ws_id, "auth-module").await.unwrap();
        assert!(page.is_some());
        let page = page.unwrap();
        assert_eq!(page.title, "认证模块");
        assert_eq!(page.content, "# 认证模块\n\n内容");
        assert_eq!(page.page_type, PAGE_TYPE_TOPIC);
    }

    /// 验证 upsert 更新已有页面时覆盖 content/title，合并 source_refs。
    #[tokio::test]
    async fn test_upsert_updates_and_merges_refs() {
        let db = Database::new(":memory:").await.expect("db must open");
        let ws_id = create_test_workspace(&db).await;

        // 第一次创建
        db.upsert_blackboard_page(
            ws_id,
            PAGE_TYPE_TOPIC,
            "auth",
            "认证",
            "认证相关结论",
            "初始内容",
            &[1, 2],
        )
        .await
        .unwrap();

        // 第二次更新（同 slug）
        db.upsert_blackboard_page(
            ws_id,
            PAGE_TYPE_TOPIC,
            "auth",
            "认证模块",
            "认证模块相关结论",
            "更新内容",
            &[2, 3],
        )
        .await
        .unwrap();

        let page = db.get_blackboard_page(ws_id, "auth").await.unwrap().unwrap();
        assert_eq!(page.title, "认证模块", "title 应被覆盖");
        assert_eq!(page.content, "更新内容", "content 应被覆盖");
        // source_refs 应合并去重：[1,2] + [2,3] = [1,2,3]
        let refs: Vec<i64> = serde_json::from_str(&page.source_refs).unwrap();
        assert_eq!(refs, vec![1, 2, 3], "source_refs 应合并去重");
    }

    /// 验证 list_topic_pages 只返回 topic 类型页面。
    #[tokio::test]
    async fn test_list_topic_pages_filters_by_type() {
        let db = Database::new(":memory:").await.expect("db must open");
        let ws_id = create_test_workspace(&db).await;

        // 创建三种类型的页面
        db.upsert_blackboard_page(ws_id, PAGE_TYPE_INDEX, "index", "目录", "知识库目录", "目录内容", &[])
            .await
            .unwrap();
        db.upsert_blackboard_page(ws_id, PAGE_TYPE_TOPIC, "auth", "认证", "认证相关", "认证内容", &[])
            .await
            .unwrap();
        db.upsert_blackboard_page(ws_id, PAGE_TYPE_LOG, "log", "日志", "更新日志", "日志内容", &[])
            .await
            .unwrap();

        let topics = db.list_topic_pages(ws_id).await.unwrap();
        assert_eq!(topics.len(), 1, "只应返回 1 个 topic 页面");
        assert_eq!(topics[0].slug, "auth");
    }

    /// 验证 append_log_entry 在无 log 页面时创建，有时追加。
    #[tokio::test]
    async fn test_append_log_entry_creates_then_appends() {
        let db = Database::new(":memory:").await.expect("db must open");
        let ws_id = create_test_workspace(&db).await;

        // 第一次：创建 log 页面
        db.append_log_entry(ws_id, "## 第一条日志")
            .await
            .unwrap();
        let log = db.get_blackboard_page(ws_id, PAGE_TYPE_LOG).await.unwrap();
        assert!(log.is_some());
        assert_eq!(log.as_ref().unwrap().content, "## 第一条日志");

        // 第二次：追加
        db.append_log_entry(ws_id, "## 第二条日志")
            .await
            .unwrap();
        let log = db.get_blackboard_page(ws_id, PAGE_TYPE_LOG).await.unwrap().unwrap();
        assert!(
            log.content.contains("第一条日志") && log.content.contains("第二条日志"),
            "log 内容应包含两条日志"
        );
    }

    /// 验证 delete_blackboard_page 删除指定页面。
    #[tokio::test]
    async fn test_delete_blackboard_page() {
        let db = Database::new(":memory:").await.expect("db must open");
        let ws_id = create_test_workspace(&db).await;

        db.upsert_blackboard_page(ws_id, PAGE_TYPE_TOPIC, "temp", "临时", "临时页面", "内容", &[])
            .await
            .unwrap();
        assert!(db.get_blackboard_page(ws_id, "temp").await.unwrap().is_some());

        db.delete_blackboard_page(ws_id, "temp").await.unwrap();
        assert!(db.get_blackboard_page(ws_id, "temp").await.unwrap().is_none());
    }

    /// 验证不同 workspace 的页面隔离：slug 相同但 workspace 不同是两条记录。
    #[tokio::test]
    async fn test_pages_isolated_per_workspace() {
        let db = Database::new(":memory:").await.expect("db must open");
        let ws1 = db
            .create_project_directory("/tmp/test-bp-ws1", None, false, false)
            .await
            .unwrap();
        let ws2 = db
            .create_project_directory("/tmp/test-bp-ws2", None, false, false)
            .await
            .unwrap();

        db.upsert_blackboard_page(ws1, PAGE_TYPE_TOPIC, "auth", "认证1", "认证相关", "内容1", &[])
            .await
            .unwrap();
        db.upsert_blackboard_page(ws2, PAGE_TYPE_TOPIC, "auth", "认证2", "认证相关", "内容2", &[])
            .await
            .unwrap();

        let p1 = db.get_blackboard_page(ws1, "auth").await.unwrap().unwrap();
        let p2 = db.get_blackboard_page(ws2, "auth").await.unwrap().unwrap();
        assert_ne!(p1.id, p2.id, "不同 workspace 的页面应是独立记录");
        assert_ne!(p1.content, p2.content);
    }
}
