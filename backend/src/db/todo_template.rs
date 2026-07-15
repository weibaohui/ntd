use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, EntityTrait, QueryFilter, QueryOrder,
};

use crate::db::Database;
use crate::db::entity::todo_templates;
use crate::models::TodoTemplate;

pub struct TemplateInput<'a> {
    pub title: &'a str,
    pub prompt: Option<&'a str>,
    pub category: &'a str,
    pub sort_order: Option<i32>,
}

impl Database {
    pub async fn get_template_by_id(&self, id: i64) -> Result<Option<TodoTemplate>, sea_orm::DbErr> {
        let model = todo_templates::Entity::find_by_id(id)
            .one(&self.conn)
            .await?;
        Ok(model.map(|m| TodoTemplate {
            id: m.id,
            title: m.title,
            prompt: m.prompt,
            category: m.category,
            sort_order: m.sort_order.unwrap_or(0),
            is_system: m.is_system,
            source_url: m.source_url,
            last_sync_at: m.last_sync_at,
            created_at: m.created_at,
            updated_at: m.updated_at,
        }))
    }

    pub async fn get_templates(&self) -> Result<Vec<TodoTemplate>, sea_orm::DbErr> {
        let models = todo_templates::Entity::find()
            .order_by_asc(todo_templates::Column::SortOrder)
            .order_by_asc(todo_templates::Column::Id)
            .all(&self.conn)
            .await?;
        Ok(models
            .into_iter()
            .map(|m| TodoTemplate {
                id: m.id,
                title: m.title,
                prompt: m.prompt,
                category: m.category,
                sort_order: m.sort_order.unwrap_or(0),
                is_system: m.is_system,
                source_url: m.source_url,
                last_sync_at: m.last_sync_at,
                created_at: m.created_at,
                updated_at: m.updated_at,
            })
            .collect())
    }

    pub async fn get_templates_by_category(&self, category: &str) -> Result<Vec<TodoTemplate>, sea_orm::DbErr> {
        let models = todo_templates::Entity::find()
            .filter(todo_templates::Column::Category.eq(category.to_string()))
            .order_by_asc(todo_templates::Column::SortOrder)
            .order_by_asc(todo_templates::Column::Id)
            .all(&self.conn)
            .await?;
        Ok(models
            .into_iter()
            .map(|m| TodoTemplate {
                id: m.id,
                title: m.title,
                prompt: m.prompt,
                category: m.category,
                sort_order: m.sort_order.unwrap_or(0),
                is_system: m.is_system,
                source_url: m.source_url,
                last_sync_at: m.last_sync_at,
                created_at: m.created_at,
                updated_at: m.updated_at,
            })
            .collect())
    }

    pub async fn create_template(
        &self,
        input: TemplateInput<'_>,
        is_system: bool,
    ) -> Result<i64, sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let am = todo_templates::ActiveModel {
            title: ActiveValue::Set(input.title.to_string()),
            prompt: ActiveValue::Set(input.prompt.map(String::from)),
            category: ActiveValue::Set(input.category.to_string()),
            sort_order: ActiveValue::Set(input.sort_order),
            is_system: ActiveValue::Set(is_system),
            source_url: ActiveValue::Set(None),
            last_sync_at: ActiveValue::Set(None),
            created_at: ActiveValue::Set(Some(now.clone())),
            updated_at: ActiveValue::Set(Some(now)),
            ..Default::default()
        };
        let inserted = am.insert(&self.conn).await?;
        Ok(inserted.id)
    }

    /// Create a custom template from remote URL sync
    pub async fn create_template_from_remote(
        &self,
        input: TemplateInput<'_>,
        source_url: &str,
    ) -> Result<i64, sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let am = todo_templates::ActiveModel {
            title: ActiveValue::Set(input.title.to_string()),
            prompt: ActiveValue::Set(input.prompt.map(String::from)),
            category: ActiveValue::Set(input.category.to_string()),
            sort_order: ActiveValue::Set(input.sort_order),
            is_system: ActiveValue::Set(false),
            source_url: ActiveValue::Set(Some(source_url.to_string())),
            last_sync_at: ActiveValue::Set(Some(now.clone())),
            created_at: ActiveValue::Set(Some(now.clone())),
            updated_at: ActiveValue::Set(Some(now)),
            ..Default::default()
        };
        let inserted = am.insert(&self.conn).await?;
        Ok(inserted.id)
    }

    /// Get custom template subscription info (templates with source_url set)
    pub async fn get_custom_template_subscription(&self) -> Result<Option<(String, Option<String>)>, sea_orm::DbErr> {
        let model = todo_templates::Entity::find()
            .filter(todo_templates::Column::SourceUrl.is_not_null())
            .order_by_desc(todo_templates::Column::UpdatedAt)
            .one(&self.conn)
            .await?;
        // is_not_null 过滤保证 source_url 必定有值
        Ok(model.and_then(|m| m.source_url.map(|url| (url, m.last_sync_at))))
    }

    /// Delete all templates that came from a specific remote URL (for re-sync)
    pub async fn delete_templates_by_source_url(&self, source_url: &str) -> Result<u64, sea_orm::DbErr> {
        let count = todo_templates::Entity::delete_many()
            .filter(todo_templates::Column::SourceUrl.eq(source_url.to_string()))
            .exec(&self.conn)
            .await?;
        Ok(count.rows_affected)
    }

    /// Delete all custom templates (where source_url is not null)
    pub async fn delete_all_custom_templates(&self) -> Result<u64, sea_orm::DbErr> {
        let count = todo_templates::Entity::delete_many()
            .filter(todo_templates::Column::SourceUrl.is_not_null())
            .exec(&self.conn)
            .await?;
        Ok(count.rows_affected)
    }

    pub async fn update_template(
        &self,
        id: i64,
        input: TemplateInput<'_>,
    ) -> Result<(), sea_orm::DbErr> {
        let model = todo_templates::Entity::find_by_id(id)
            .one(&self.conn)
            .await?
            .ok_or_else(|| sea_orm::DbErr::RecordNotFound("Template not found".to_string()))?;

        let mut am: todo_templates::ActiveModel = model.into();
        am.title = ActiveValue::Set(input.title.to_string());
        am.prompt = ActiveValue::Set(input.prompt.map(String::from));
        am.category = ActiveValue::Set(input.category.to_string());
        am.sort_order = ActiveValue::Set(input.sort_order);
        am.updated_at = ActiveValue::Set(Some(crate::models::utc_timestamp()));
        am.update(&self.conn).await?;
        Ok(())
    }

    pub async fn delete_template(&self, id: i64) -> Result<(), sea_orm::DbErr> {
        todo_templates::Entity::delete_by_id(id).exec(&self.conn).await?;
        Ok(())
    }

    /// Upsert 系统模板（从 bundled 同步）
    ///
    /// 通过 source_url 定位已存在的系统模板，存在则更新、不存在则插入。
    /// 仅修改系统字段（title/prompt/category/sort_order），不会覆盖用户已修改的本地数据。
    #[allow(clippy::too_many_arguments)]
    pub async fn upsert_system_template(
        &self,
        _template_id: &str,
        title: &str,
        prompt: Option<&str>,
        category: &str,
        sort_order: Option<i32>,
        source_url: &str,
        last_sync_at: &str,
    ) -> Result<(), sea_orm::DbErr> {
        // 查找已存在的同 source_url 模板
        let existing = todo_templates::Entity::find()
            .filter(todo_templates::Column::SourceUrl.eq(source_url.to_string()))
            .one(&self.conn)
            .await?;

        if let Some(m) = existing {
            // 已存在则更新
            let mut am: todo_templates::ActiveModel = m.into();
            am.title = ActiveValue::Set(title.to_string());
            am.prompt = ActiveValue::Set(prompt.map(String::from));
            am.category = ActiveValue::Set(category.to_string());
            am.sort_order = ActiveValue::Set(sort_order);
            am.last_sync_at = ActiveValue::Set(Some(last_sync_at.to_string()));
            am.updated_at = ActiveValue::Set(Some(crate::models::utc_timestamp()));
            am.update(&self.conn).await?;
        } else {
            // 不存在则插入
            let now = crate::models::utc_timestamp();
            let am = todo_templates::ActiveModel {
                title: ActiveValue::Set(title.to_string()),
                prompt: ActiveValue::Set(prompt.map(String::from)),
                category: ActiveValue::Set(category.to_string()),
                sort_order: ActiveValue::Set(sort_order),
                is_system: ActiveValue::Set(true),
                source_url: ActiveValue::Set(Some(source_url.to_string())),
                last_sync_at: ActiveValue::Set(Some(last_sync_at.to_string())),
                created_at: ActiveValue::Set(Some(now.clone())),
                updated_at: ActiveValue::Set(Some(now)),
                ..Default::default()
            };
            am.insert(&self.conn).await?;
        }
        Ok(())
    }
}
