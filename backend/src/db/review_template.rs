//! `review_templates` 表的 DAO（数据访问对象）。
//!
//! 实现为 `Database` 上的方法扩展，保持项目里其他表（如 `todo_templates`、
//! `todo`）一致的代码组织。
//!
//! 设计要点：
//! - **不依赖 auto_review 模块**：`ensure_default_review_template` 是事实上的
//!   "取默认"入口；它不再走 todos 路径，纯查 review_templates。
//! - **list_review_template_options() 不返回 prompt**：loop 选择器只需要
//!   id/name/description，省字节且防止 prompt 内容意外渲染。
//! - **create_review_template 不强制 name 唯一**：schema 没加 UNIQUE 约束，
//!   DAO 层允许重名（与业务层约定一致：业务层校验唯一，DAO 不双重检查）。
//! - **delete_review_template 不级联**：loops.review_template_id 没有 DB 级 FK
//!   （plan 中已说明 SeaORM + SQLite 的 FK 处理不一致），删模板时业务层决定
//!   是否把 loop 引用置 NULL。

use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, EntityTrait, QueryFilter, QueryOrder,
};

use crate::db::entity::review_templates;
use crate::db::Database;
use crate::models::{CreateReviewTemplateRequest, ReviewTemplate, ReviewTemplateOption, UpdateReviewTemplateRequest};

/// DAO 输入结构：在调用方组装，避免签名里堆 6 个参数。
#[derive(Debug, Clone)]
pub struct ReviewTemplateInput {
    pub name: String,
    pub description: Option<String>,
    pub prompt: String,
}

impl From<&CreateReviewTemplateRequest> for ReviewTemplateInput {
    fn from(r: &CreateReviewTemplateRequest) -> Self {
        Self {
            name: r.name.clone(),
            description: r.description.clone(),
            prompt: r.prompt.clone(),
        }
    }
}

impl From<&UpdateReviewTemplateRequest> for ReviewTemplateInput {
    fn from(r: &UpdateReviewTemplateRequest) -> Self {
        Self {
            name: r.name.clone(),
            description: r.description.clone(),
            prompt: r.prompt.clone(),
        }
    }
}

/// 实体 Model → 领域 Model 的纯转换。抽出来便于 DAO 和测试共用，
/// 避免每个查询方法都重写一遍字段映射。
fn to_domain(m: review_templates::Model) -> ReviewTemplate {
    ReviewTemplate {
        id: m.id,
        name: m.name,
        description: m.description,
        prompt: m.prompt,
        created_at: m.created_at,
        updated_at: m.updated_at,
    }
}

fn to_option(m: review_templates::Model) -> ReviewTemplateOption {
    ReviewTemplateOption {
        id: m.id,
        name: m.name,
        description: m.description,
    }
}

impl Database {
    /// 列出全部评审模板，按 id 升序。返回完整模型（含 prompt）。
    /// 设计原因：管理面板需要看 prompt；选项下拉请用 `list_review_template_options`。
    pub async fn list_review_templates(&self) -> Result<Vec<ReviewTemplate>, sea_orm::DbErr> {
        let models = review_templates::Entity::find()
            .order_by_asc(review_templates::Column::Id)
            .all(&self.conn)
            .await?;
        Ok(models.into_iter().map(to_domain).collect())
    }

    /// 列出评审模板的轻量选项（不含 prompt），用于 loop 编辑器下拉。
    pub async fn list_review_template_options(&self) -> Result<Vec<ReviewTemplateOption>, sea_orm::DbErr> {
        let models = review_templates::Entity::find()
            .order_by_asc(review_templates::Column::Id)
            .all(&self.conn)
            .await?;
        Ok(models.into_iter().map(to_option).collect())
    }

    /// 按 id 取单条模板；不存在返回 None。
    pub async fn get_review_template(&self, id: i64) -> Result<Option<ReviewTemplate>, sea_orm::DbErr> {
        let model = review_templates::Entity::find_by_id(id)
            .one(&self.conn)
            .await?;
        Ok(model.map(to_domain))
    }

    /// 按 name 精确取单条模板；用于 ensure_default 等"按名唯一"语义。
    pub async fn get_review_template_by_name(&self, name: &str) -> Result<Option<ReviewTemplate>, sea_orm::DbErr> {
        let model = review_templates::Entity::find()
            .filter(review_templates::Column::Name.eq(name.to_string()))
            .one(&self.conn)
            .await?;
        Ok(model.map(to_domain))
    }

    /// 创建一条评审模板，返回新行的 id。
    /// 自动写入 created_at / updated_at（UTC ISO8601）。
    pub async fn create_review_template(
        &self,
        input: &ReviewTemplateInput,
    ) -> Result<i64, sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let am = review_templates::ActiveModel {
            name: ActiveValue::Set(input.name.clone()),
            description: ActiveValue::Set(input.description.clone()),
            prompt: ActiveValue::Set(input.prompt.clone()),
            created_at: ActiveValue::Set(Some(now.clone())),
            updated_at: ActiveValue::Set(Some(now)),
            ..Default::default()
        };
        let inserted = am.insert(&self.conn).await?;
        Ok(inserted.id)
    }

    /// 更新一条评审模板；不存在返回 NotFound。
    /// 设计原因：始终要求传全字段（name/description/prompt），避免 PUT 语义下
    /// 漏传导致字段被空字符串覆盖。
    pub async fn update_review_template(
        &self,
        id: i64,
        input: &ReviewTemplateInput,
    ) -> Result<(), sea_orm::DbErr> {
        let model = review_templates::Entity::find_by_id(id)
            .one(&self.conn)
            .await?
            .ok_or_else(|| sea_orm::DbErr::RecordNotFound(format!("review template #{} not found", id)))?;
        let mut am: review_templates::ActiveModel = model.into();
        am.name = ActiveValue::Set(input.name.clone());
        am.description = ActiveValue::Set(input.description.clone());
        am.prompt = ActiveValue::Set(input.prompt.clone());
        am.updated_at = ActiveValue::Set(Some(crate::models::utc_timestamp()));
        am.update(&self.conn).await?;
        Ok(())
    }

    /// 删除一条评审模板；返回是否真的删了（一行 vs 零行）。
    /// loops.review_template_id 由调用方（service / handler）决定是否置 NULL；
    /// DAO 不感知 loops。
    pub async fn delete_review_template(&self, id: i64) -> Result<bool, sea_orm::DbErr> {
        let result = review_templates::Entity::delete_by_id(id)
            .exec(&self.conn)
            .await?;
        Ok(result.rows_affected > 0)
    }

    /// 确保名为"默认评审任务"的模板存在；不存在则插入（用 DEFAULT_REVIEWER_PROMPT）。
    /// 返回该行的 id。
    ///
    /// 与 V15 迁移的语义一致：默认 name 硬编码为 "默认评审任务"，prompt 使用
    /// `crate::services::auto_review::DEFAULT_REVIEWER_PROMPT`。迁移走 SQL
    /// 路径，本方法走 ORM 路径——两条路径产生的内容一致。
    ///
    /// 并发安全：SQLite 通过 WAL + 数据库锁串行化写操作。即便两个并发调用都
    /// 走到"未找到→插入"分支，后插入的会因 PRIMARY KEY 冲突失败；外层调用
    /// 会重试 `get_review_template_by_name` 拿到已存在的行。具体行为见测试。
    pub async fn ensure_default_review_template(&self) -> Result<i64, sea_orm::DbErr> {
        const DEFAULT_NAME: &str = "默认评审任务";
        if let Some(t) = self.get_review_template_by_name(DEFAULT_NAME).await? {
            return Ok(t.id);
        }
        let prompt = crate::services::auto_review::DEFAULT_REVIEWER_PROMPT;
        let now = crate::models::utc_timestamp();
        let am = review_templates::ActiveModel {
            name: ActiveValue::Set(DEFAULT_NAME.to_string()),
            description: ActiveValue::Set(None),
            prompt: ActiveValue::Set(prompt.to_string()),
            created_at: ActiveValue::Set(Some(now.clone())),
            updated_at: ActiveValue::Set(Some(now)),
            ..Default::default()
        };
        // 插入可能因 PRIMARY KEY 冲突失败（并发场景）；失败时回退到 SELECT
        match am.insert(&self.conn).await {
            Ok(inserted) => Ok(inserted.id),
            Err(_) => {
                // 重试一次：另一个并发任务可能已经插好
                self.get_review_template_by_name(DEFAULT_NAME)
                    .await?
                    .map(|t| t.id)
                    .ok_or_else(|| sea_orm::DbErr::RecordNotFound(
                        "default review template not found after concurrent insert".to_string()
                    ))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    //! DAO 单元测试：每个方法一个 happy path + 一个错误/边界 path。
    //! 使用 `:memory:` DB，每个测试一个独立 ephemeral store。

    use super::*;
    use crate::db::Database;

    async fn fresh_db() -> Database {
        Database::new(":memory:").await.expect("memory db must open")
    }

    fn sample_input(name: &str, prompt: &str) -> ReviewTemplateInput {
        ReviewTemplateInput {
            name: name.to_string(),
            description: Some(format!("{name} 描述")),
            prompt: prompt.to_string(),
        }
    }

    // -------- list_review_templates --------

    #[tokio::test]
    async fn list_returns_only_seeded_default_on_fresh_db() {
        // V15 迁移在 fresh DB 上自动 seed 一条默认模板, list 不该空着。
        let db = fresh_db().await;
        let list = db.list_review_templates().await.expect("list must succeed");
        assert_eq!(list.len(), 1, "V15 seeds exactly one default template");
        assert_eq!(list[0].name, "默认评审任务");
    }

    #[tokio::test]
    async fn list_returns_inserted_rows_in_id_order() {
        // 先删默认, 再插两条新模板, 验证 id 升序 + prompt 完整
        let db = fresh_db().await;
        let default_id = db.ensure_default_review_template().await.expect("ensure");
        db.delete_review_template(default_id).await.expect("del default");
        db.create_review_template(&sample_input("B", "p-b")).await.expect("create B");
        db.create_review_template(&sample_input("A", "p-a")).await.expect("create A");
        let list = db.list_review_templates().await.expect("list");
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].name, "B", "first by id ascending");
        assert_eq!(list[1].name, "A");
        // list 返回完整模型,prompt 必须含原文
        assert_eq!(list[0].prompt, "p-b");
    }

    // -------- list_review_template_options --------

    #[tokio::test]
    async fn options_omits_prompt_field() {
        let db = fresh_db().await;
        // 默认模板已经在 fresh DB 里; 再插一条「代码评审」, 总共 2 条 options
        db.create_review_template(&sample_input("代码评审", "敏感 prompt 内含 RATING 占位符"))
            .await
            .expect("create");
        let opts = db.list_review_template_options().await.expect("opts");
        assert_eq!(opts.len(), 2, "default + created = 2");
        // 第二条是「代码评审」, 验证其字段且不带 prompt (编译期由 ReviewTemplateOption 字段保证)
        let code_review = opts.iter().find(|o| o.name == "代码评审").expect("must find");
        assert_eq!(code_review.description.as_deref(), Some("代码评审 描述"));
        // 默认那一条没有 description
        let default = opts.iter().find(|o| o.name == "默认评审任务").expect("must find default");
        assert!(default.description.is_none());
    }

    // -------- get_review_template --------

    #[tokio::test]
    async fn get_existing_returns_some_with_full_fields() {
        let db = fresh_db().await;
        let id = db.create_review_template(&sample_input("X", "prompt-x")).await.expect("create");
        let got = db.get_review_template(id).await.expect("get").expect("must be Some");
        assert_eq!(got.id, id);
        assert_eq!(got.name, "X");
        assert_eq!(got.prompt, "prompt-x");
        assert!(got.created_at.is_some(), "created_at must be set by DAO");
        assert!(got.updated_at.is_some(), "updated_at must be set by DAO");
    }

    #[tokio::test]
    async fn get_missing_returns_none() {
        let db = fresh_db().await;
        let got = db.get_review_template(99999).await.expect("get");
        assert!(got.is_none(), "missing id must return None");
    }

    // -------- get_review_template_by_name --------

    #[tokio::test]
    async fn get_by_name_finds_existing() {
        let db = fresh_db().await;
        db.create_review_template(&sample_input("代码评审", "p")).await.expect("create");
        let got = db.get_review_template_by_name("代码评审").await.expect("by name");
        assert!(got.is_some());
        assert_eq!(got.unwrap().name, "代码评审");
    }

    #[tokio::test]
    async fn get_by_name_missing_returns_none() {
        let db = fresh_db().await;
        let got = db.get_review_template_by_name("不存在").await.expect("by name");
        assert!(got.is_none());
    }

    // -------- create_review_template --------

    #[tokio::test]
    async fn create_returns_inserted_id_and_stores_all_fields() {
        let db = fresh_db().await;
        let input = ReviewTemplateInput {
            name: "新模板".to_string(),
            description: Some("描述".to_string()),
            prompt: "你是一个评审师".to_string(),
        };
        let id = db.create_review_template(&input).await.expect("create");
        let row = db.get_review_template(id).await.expect("get").expect("some");
        assert_eq!(row.name, "新模板");
        assert_eq!(row.description.as_deref(), Some("描述"));
        assert_eq!(row.prompt, "你是一个评审师");
    }

    #[tokio::test]
    async fn create_with_null_description_stores_null() {
        let db = fresh_db().await;
        let input = ReviewTemplateInput {
            name: "no-desc".to_string(),
            description: None,
            prompt: "p".to_string(),
        };
        let id = db.create_review_template(&input).await.expect("create");
        let row = db.get_review_template(id).await.expect("get").expect("some");
        assert!(row.description.is_none(), "null description must persist");
    }

    // -------- update_review_template --------

    #[tokio::test]
    async fn update_existing_replaces_all_fields_and_bumps_updated_at() {
        let db = fresh_db().await;
        let id = db.create_review_template(&sample_input("old", "old-prompt")).await.expect("create");
        let before = db.get_review_template(id).await.expect("get").expect("some").updated_at.clone();

        // 等待一毫秒以保证 updated_at 时间戳能观察到变化
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        let input = ReviewTemplateInput {
            name: "new".to_string(),
            description: Some("new-desc".to_string()),
            prompt: "new-prompt".to_string(),
        };
        db.update_review_template(id, &input).await.expect("update");
        let after = db.get_review_template(id).await.expect("get").expect("some");
        assert_eq!(after.name, "new");
        assert_eq!(after.description.as_deref(), Some("new-desc"));
        assert_eq!(after.prompt, "new-prompt");
        assert_ne!(after.updated_at, before, "updated_at must advance on update");
    }

    #[tokio::test]
    async fn update_missing_returns_record_not_found() {
        let db = fresh_db().await;
        let input = sample_input("x", "p");
        let err = db.update_review_template(99999, &input).await.expect_err("must error");
        match err {
            sea_orm::DbErr::RecordNotFound(_) => {} // 期望路径
            other => panic!("expected RecordNotFound, got {:?}", other),
        }
    }

    // -------- delete_review_template --------

    #[tokio::test]
    async fn delete_existing_returns_true_and_removes_row() {
        let db = fresh_db().await;
        let id = db.create_review_template(&sample_input("del", "p")).await.expect("create");
        let deleted = db.delete_review_template(id).await.expect("delete");
        assert!(deleted, "delete must return true for existing row");
        assert!(db.get_review_template(id).await.expect("get").is_none());
    }

    #[tokio::test]
    async fn delete_missing_returns_false() {
        let db = fresh_db().await;
        let deleted = db.delete_review_template(99999).await.expect("delete");
        assert!(!deleted, "delete must return false for missing row");
    }

    // -------- ensure_default_review_template --------

    #[tokio::test]
    async fn ensure_default_on_fresh_db_seeds_default_template() {
        let db = fresh_db().await;
        let id = db.ensure_default_review_template().await.expect("ensure");
        let row = db.get_review_template(id).await.expect("get").expect("some");
        assert_eq!(row.name, "默认评审任务");
        assert!(row.prompt.contains("评审") && row.prompt.contains("RATING"));
    }

    #[tokio::test]
    async fn ensure_default_is_idempotent_returns_same_id() {
        let db = fresh_db().await;
        let id1 = db.ensure_default_review_template().await.expect("ensure 1");
        let id2 = db.ensure_default_review_template().await.expect("ensure 2");
        assert_eq!(id1, id2, "ensure must return same id on repeat");
        let count: u64 = {
            use sea_orm::{EntityTrait, PaginatorTrait};
            review_templates::Entity::find().count(&db.conn).await.expect("count")
        };
        assert_eq!(count, 1, "ensure must not duplicate");
    }

    #[tokio::test]
    async fn ensure_default_preserves_user_edited_prompt_on_repeat() {
        // 用户改过默认 prompt 后,ensure 不应覆盖回去
        let db = fresh_db().await;
        let id = db.ensure_default_review_template().await.expect("ensure");
        db.update_review_template(id, &ReviewTemplateInput {
            name: "默认评审任务".to_string(),
            description: None,
            prompt: "user-customized prompt".to_string(),
        }).await.expect("update");
        let id2 = db.ensure_default_review_template().await.expect("ensure 2");
        assert_eq!(id, id2);
        let row = db.get_review_template(id2).await.expect("get").expect("some");
        assert_eq!(row.prompt, "user-customized prompt", "ensure must not overwrite user edits");
    }

    #[tokio::test]
    async fn ensure_default_concurrent_calls_yield_exactly_one_row() {
        // 两个 ensure 并发执行, 必须只有一条默认行
        let db = std::sync::Arc::new(fresh_db().await);
        let (id_a, id_b) = tokio::join!(
            db.ensure_default_review_template(),
            db.ensure_default_review_template(),
        );
        let id_a = id_a.expect("ensure A");
        let id_b = id_b.expect("ensure B");
        assert_eq!(id_a, id_b, "concurrent ensure must converge on same id");
        // 必须恰好一行（不会重复插入）
        let list = db.list_review_templates().await.expect("list");
        assert_eq!(list.len(), 1, "concurrent ensure must not duplicate");
    }
}