use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, EntityTrait, QueryFilter, QueryOrder,
};

use crate::db::Database;
use crate::db::entity::{process_step_templates, process_templates};

impl Database {
    /// 按 ID 查找工艺模板。
    pub async fn get_process_template_by_id(
        &self,
        id: i64,
    ) -> Result<Option<process_templates::Model>, sea_orm::DbErr> {
        process_templates::Entity::find_by_id(id).one(&self.conn).await
    }

    /// 按名称查找工艺模板。
    pub async fn get_process_template_by_name(
        &self,
        name: &str,
    ) -> Result<Option<process_templates::Model>, sea_orm::DbErr> {
        process_templates::Entity::find()
            .filter(process_templates::Column::Name.eq(name.to_string()))
            .one(&self.conn)
            .await
    }

    /// 列出全部工艺模板，按名称升序。
    pub async fn list_process_templates(
        &self,
    ) -> Result<Vec<process_templates::Model>, sea_orm::DbErr> {
        process_templates::Entity::find()
            .order_by_asc(process_templates::Column::Name)
            .all(&self.conn)
            .await
    }

    /// Upsert 系统工艺模板（从 bundled 同步）。
    ///
    /// 以 `name` 为唯一键：存在则更新，不存在则插入。
    /// 工艺正文（YAML）不再落库，仅存 `source_path` 引用；内容按路径从磁盘文件读取。
    #[allow(clippy::too_many_arguments)]
    pub async fn upsert_system_process_template(
        &self,
        name: &str,
        display_name: &str,
        description: &str,
        category: &str,
        complexity: &str,
        version: &str,
        source_path: &str,
    ) -> Result<i64, sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let existing = process_templates::Entity::find()
            .filter(process_templates::Column::Name.eq(name.to_string()))
            .one(&self.conn)
            .await?;

        if let Some(m) = existing {
            let mut am: process_templates::ActiveModel = m.into();
            am.display_name = ActiveValue::Set(display_name.to_string());
            am.description = ActiveValue::Set(description.to_string());
            am.category = ActiveValue::Set(category.to_string());
            am.complexity = ActiveValue::Set(complexity.to_string());
            am.version = ActiveValue::Set(version.to_string());
            am.source_path = ActiveValue::Set(Some(source_path.to_string()));
            am.is_system = ActiveValue::Set(true);
            am.updated_at = ActiveValue::Set(Some(now));
            let updated = am.update(&self.conn).await?;
            Ok(updated.id)
        } else {
            let am = process_templates::ActiveModel {
                name: ActiveValue::Set(name.to_string()),
                display_name: ActiveValue::Set(display_name.to_string()),
                description: ActiveValue::Set(description.to_string()),
                category: ActiveValue::Set(category.to_string()),
                complexity: ActiveValue::Set(complexity.to_string()),
                version: ActiveValue::Set(version.to_string()),
                source_path: ActiveValue::Set(Some(source_path.to_string())),
                workspace_id: ActiveValue::Set(None),
                is_system: ActiveValue::Set(true),
                created_at: ActiveValue::Set(Some(now.clone())),
                updated_at: ActiveValue::Set(Some(now)),
                ..Default::default()
            };
            let inserted = am.insert(&self.conn).await?;
            Ok(inserted.id)
        }
    }

    /// 按名称查找工艺环节原型。
    pub async fn get_process_step_template_by_name(
        &self,
        name: &str,
    ) -> Result<Option<process_step_templates::Model>, sea_orm::DbErr> {
        process_step_templates::Entity::find()
            .filter(process_step_templates::Column::Name.eq(name.to_string()))
            .one(&self.conn)
            .await
    }

    /// Upsert 系统工艺环节原型（从 bundled 同步）。
    #[allow(clippy::too_many_arguments)]
    pub async fn upsert_system_process_step_template(
        &self,
        name: &str,
        title: &str,
        prompt: &str,
        executor: Option<&str>,
        expert_name: Option<&str>,
        skill_names: &str,
        model: Option<&str>,
        acceptance_criteria: &str,
        source_path: &str,
        category: &str,
    ) -> Result<i64, sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let existing = process_step_templates::Entity::find()
            .filter(process_step_templates::Column::Name.eq(name.to_string()))
            .one(&self.conn)
            .await?;

        if let Some(m) = existing {
            let mut am: process_step_templates::ActiveModel = m.into();
            am.title = ActiveValue::Set(title.to_string());
            am.prompt = ActiveValue::Set(prompt.to_string());
            am.executor = ActiveValue::Set(executor.map(String::from));
            am.expert_name = ActiveValue::Set(expert_name.map(String::from));
            am.skill_names = ActiveValue::Set(skill_names.to_string());
            am.model = ActiveValue::Set(model.map(String::from));
            am.acceptance_criteria = ActiveValue::Set(acceptance_criteria.to_string());
            am.category = ActiveValue::Set(category.to_string());
            am.source_path = ActiveValue::Set(Some(source_path.to_string()));
            am.is_system = ActiveValue::Set(true);
            am.updated_at = ActiveValue::Set(Some(now));
            let updated = am.update(&self.conn).await?;
            Ok(updated.id)
        } else {
            let am = process_step_templates::ActiveModel {
                name: ActiveValue::Set(name.to_string()),
                title: ActiveValue::Set(title.to_string()),
                prompt: ActiveValue::Set(prompt.to_string()),
                executor: ActiveValue::Set(executor.map(String::from)),
                expert_name: ActiveValue::Set(expert_name.map(String::from)),
                skill_names: ActiveValue::Set(skill_names.to_string()),
                model: ActiveValue::Set(model.map(String::from)),
                acceptance_criteria: ActiveValue::Set(acceptance_criteria.to_string()),
                category: ActiveValue::Set(category.to_string()),
                workspace_id: ActiveValue::Set(None),
                is_system: ActiveValue::Set(true),
                source_path: ActiveValue::Set(Some(source_path.to_string())),
                created_at: ActiveValue::Set(Some(now.clone())),
                updated_at: ActiveValue::Set(Some(now)),
                ..Default::default()
            };
            let inserted = am.insert(&self.conn).await?;
            Ok(inserted.id)
        }
    }

    /// Upsert 用户工艺模板（从 `~/.ntd/processes/` 扫描）。
    ///
    /// 与 `upsert_system_process_template` 的区别：
    /// - `is_system=false`，标记为用户自定义工艺
    /// - `workspace_id=NULL`，本需求先支持全局用户工艺
    /// - 同名工艺覆盖系统层（`name` 为唯一键，第二次 upsert 改写 `is_system` 从 true 变 false）
    ///
    /// 工艺正文只存于磁盘（~/.ntd/processes/），DB 仅存 `source_path` 引用。
    #[allow(clippy::too_many_arguments)]
    pub async fn upsert_user_process_template(
        &self,
        name: &str,
        display_name: &str,
        description: &str,
        category: &str,
        complexity: &str,
        version: &str,
        source_path: &str,
    ) -> Result<i64, sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let existing = process_templates::Entity::find()
            .filter(process_templates::Column::Name.eq(name.to_string()))
            .one(&self.conn)
            .await?;

        // 用户层覆盖系统层：无论是新增还是更新，都写入 is_system=false、workspace_id=NULL。
        if let Some(m) = existing {
            let mut am: process_templates::ActiveModel = m.into();
            am.display_name = ActiveValue::Set(display_name.to_string());
            am.description = ActiveValue::Set(description.to_string());
            am.category = ActiveValue::Set(category.to_string());
            am.complexity = ActiveValue::Set(complexity.to_string());
            am.version = ActiveValue::Set(version.to_string());
            am.source_path = ActiveValue::Set(Some(source_path.to_string()));
            am.workspace_id = ActiveValue::Set(None);
            am.is_system = ActiveValue::Set(false);
            am.updated_at = ActiveValue::Set(Some(now));
            let updated = am.update(&self.conn).await?;
            Ok(updated.id)
        } else {
            let am = process_templates::ActiveModel {
                name: ActiveValue::Set(name.to_string()),
                display_name: ActiveValue::Set(display_name.to_string()),
                description: ActiveValue::Set(description.to_string()),
                category: ActiveValue::Set(category.to_string()),
                complexity: ActiveValue::Set(complexity.to_string()),
                version: ActiveValue::Set(version.to_string()),
                source_path: ActiveValue::Set(Some(source_path.to_string())),
                workspace_id: ActiveValue::Set(None),
                is_system: ActiveValue::Set(false),
                created_at: ActiveValue::Set(Some(now.clone())),
                updated_at: ActiveValue::Set(Some(now)),
                ..Default::default()
            };
            let inserted = am.insert(&self.conn).await?;
            Ok(inserted.id)
        }
    }

    /// 删除所有系统工艺模板（`is_system=true`），保留用户工艺。
    ///
    /// "先删后插"策略：系统同步开始前调用，确保远程删除的工艺在本地也消失。
    /// 用户工艺（`is_system=false`）不受影响，保证用户自定义不被同步误删。
    pub async fn delete_all_system_process_templates(
        &self,
    ) -> Result<u64, sea_orm::DbErr> {
        let result = process_templates::Entity::delete_many()
            .filter(process_templates::Column::IsSystem.eq(true))
            .exec(&self.conn)
            .await?;
        Ok(result.rows_affected)
    }

    /// 删除所有系统环节原型（`is_system=true`），保留用户环节原型。
    ///
    /// 与 `delete_all_system_process_templates` 同步调用，保持系统层与用户层语义一致。
    pub async fn delete_all_system_process_step_templates(
        &self,
    ) -> Result<u64, sea_orm::DbErr> {
        let result = process_step_templates::Entity::delete_many()
            .filter(process_step_templates::Column::IsSystem.eq(true))
            .exec(&self.conn)
            .await?;
        Ok(result.rows_affected)
    }

    /// 按 name 删除单个工艺模板（用户工艺删除流程专用）。
    ///
    /// 与 `delete_all_system_process_templates` 的区别：
    /// - 按 `name` 精准删除单条，而非批量删除 `is_system=true` 的记录
    /// - 不区分 `is_system`，调用方需在 handler 层做来源校验
    ///   （系统工艺拒绝删除，返回 409）
    /// - 返回受影响行数：0 表示该 name 不存在（调用方应先
    ///   `get_process_template_by_name` 判 404）
    pub async fn delete_process_template(
        &self,
        name: &str,
    ) -> Result<u64, sea_orm::DbErr> {
        let result = process_templates::Entity::delete_many()
            .filter(process_templates::Column::Name.eq(name.to_string()))
            .exec(&self.conn)
            .await?;
        Ok(result.rows_affected)
    }
}
