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

    pub async fn get_task(&self, id: i64) -> Result<Option<tasks::Model>, sea_orm::DbErr> {
        tasks::Entity::find_by_id(id).one(&self.conn).await
    }

    pub async fn list_tasks(&self, status: Option<&str>) -> Result<Vec<tasks::Model>, sea_orm::DbErr> {
        let mut q = tasks::Entity::find();
        if let Some(s) = status { q = q.filter(tasks::Column::Status.eq(s)); }
        q.order_by_desc(tasks::Column::CreatedAt).all(&self.conn).await
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

    pub async fn update_task_loop_id(&self, id: i64, loop_id: i64) -> Result<(), sea_orm::DbErr> {
        let existing = tasks::Entity::find_by_id(id).one(&self.conn).await?;
        if let Some(c) = existing {
            let mut am: tasks::ActiveModel = c.into();
            am.loop_id = ActiveValue::Set(Some(loop_id));
            am.updated_at = ActiveValue::Set(Some(utc_timestamp()));
            am.update(&self.conn).await?;
        }
        Ok(())
    }
}
