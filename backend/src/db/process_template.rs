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

    /// 按 ID 批量查找工艺模板。
    ///
    /// 列表接口注入「来源工艺名称」用——一条 SQL 取回本次列表涉及的全部模板，
    /// 避免「逐 loop 调 get_process_template_by_id」的 N+1。
    /// 空入参直接返回空 Vec：filter is_in(空) 在某些后端会生成非法 SQL，提前短路更稳妥。
    pub async fn get_process_templates_by_ids(
        &self,
        ids: &[i64],
    ) -> Result<Vec<process_templates::Model>, sea_orm::DbErr> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        process_templates::Entity::find()
            .filter(process_templates::Column::Id.is_in(ids.to_vec()))
            .all(&self.conn)
            .await
    }

    /// 按名称查找工艺模板。
    ///
    /// 040 起 name 不再唯一，本函数只用于"是否存在同名"的预检（如新建工艺查重），
    /// 寻址场景一律用 `get_process_template_by_guid`。
    pub async fn get_process_template_by_name(
        &self,
        name: &str,
    ) -> Result<Option<process_templates::Model>, sea_orm::DbErr> {
        process_templates::Entity::find()
            .filter(process_templates::Column::Name.eq(name.to_string()))
            .one(&self.conn)
            .await
    }

    /// 按 guid 查找工艺模板（040：guid 是全局唯一身份，路由寻址与 reconcile 都用它）。
    pub async fn get_process_template_by_guid(
        &self,
        guid: &str,
    ) -> Result<Option<process_templates::Model>, sea_orm::DbErr> {
        process_templates::Entity::find()
            .filter(process_templates::Column::Guid.eq(guid.to_string()))
            .one(&self.conn)
            .await
    }

    /// 列出工艺模板，按名称升序。
    ///
    /// `is_system` 为 `Some` 时按系统/用户过滤（039：工艺列表「我的/模板」双视图的服务端过滤）；
    /// 为 `None` 时返回全量——统计、推荐等旧调用方需要跨两类模板聚合，不能默认过滤。
    pub async fn list_process_templates(
        &self,
        is_system: Option<bool>,
    ) -> Result<Vec<process_templates::Model>, sea_orm::DbErr> {
        let mut query = process_templates::Entity::find();
        // 只有显式传值才加过滤条件，保证 None 时 SQL 与旧行为完全一致。
        if let Some(v) = is_system {
            query = query.filter(process_templates::Column::IsSystem.eq(v));
        }
        query
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
        guid: &str,
        name: &str,
        display_name: &str,
        description: &str,
        category: &str,
        complexity: &str,
        version: &str,
        source_path: &str,
    ) -> Result<i64, sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        // 040：upsert 键从 name 改为 guid——同步更新保留原行 id，
        // loops.process_template_id 关联不再因"先删后插"被清空。
        let existing = process_templates::Entity::find()
            .filter(process_templates::Column::Guid.eq(guid.to_string()))
            .one(&self.conn)
            .await?;

        if let Some(m) = existing {
            let mut am: process_templates::ActiveModel = m.into();
            // name 也更新：远端模板改名时 guid 不变，原地改名而非新增一行。
            am.name = ActiveValue::Set(name.to_string());
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
                guid: ActiveValue::Set(guid.to_string()),
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
    ///
    /// 040 起 upsert 键为 guid：用户副本与系统模板 guid 不同，同名共存、不再互相覆盖；
    /// 用户在 YAML 里改名（guid 不变）时原地更新，不再残留旧行。
    /// 工艺正文只存于磁盘（~/.ntd/processes/），DB 仅存 `source_path` 引用。
    #[allow(clippy::too_many_arguments)]
    pub async fn upsert_user_process_template(
        &self,
        guid: &str,
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
            .filter(process_templates::Column::Guid.eq(guid.to_string()))
            .one(&self.conn)
            .await?;

        // 无论是新增还是更新，都写入 is_system=false、workspace_id=NULL（用户层语义）。
        if let Some(m) = existing {
            let mut am: process_templates::ActiveModel = m.into();
            am.name = ActiveValue::Set(name.to_string());
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
                guid: ActiveValue::Set(guid.to_string()),
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

    /// 删除不在给定 guid 集合内的系统工艺模板（040：同步 reconcile 的"删除下架"步骤）。
    ///
    /// 替代旧的"先删后插"：只删除远端仓库真正移除的模板，
    /// 仍在仓库中的模板保留原行（id 不变，loops 关联不断）。
    /// 用户工艺（`is_system=false`）不受影响。
    pub async fn delete_system_process_templates_not_in(
        &self,
        guids: &[String],
    ) -> Result<u64, sea_orm::DbErr> {
        let result = process_templates::Entity::delete_many()
            .filter(process_templates::Column::IsSystem.eq(true))
            .filter(process_templates::Column::Guid.is_not_in(guids.iter().cloned()))
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
    /// 按 guid 精准删除单条工艺模板。
    ///
    /// - 不区分 `is_system`，调用方需在 handler 层做来源校验
    ///   （系统工艺拒绝删除，返回 409）
    /// - 返回受影响行数：0 表示该 guid 不存在（调用方应先
    ///   `get_process_template_by_guid` 判 404）
    pub async fn delete_process_template(
        &self,
        guid: &str,
    ) -> Result<u64, sea_orm::DbErr> {
        let result = process_templates::Entity::delete_many()
            .filter(process_templates::Column::Guid.eq(guid.to_string()))
            .exec(&self.conn)
            .await?;
        Ok(result.rows_affected)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    /// 造一系统一用户两条模板，供过滤用例复用；
    /// name 按字典序排列（system < user），便于断言全量列表的顺序稳定性。
    async fn seed_two_templates(db: &Database) {
        db.upsert_system_process_template(
            "guid-sys-001", "sys-tpl", "系统模板", "系统", "测试", "standard", "1.0.0",
            "bundled://processes/test/sys-tpl.yaml",
        )
        .await
        .unwrap();
        db.upsert_user_process_template(
            "guid-user-001", "user-tpl", "用户模板", "用户", "测试", "light", "0.1.0",
            "user://test/user-tpl.yaml",
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn test_upsert_by_guid_rename_updates_in_place() {
        // 040：同 guid 改名应原地更新（修复旧 name 键下"YAML 改名残留旧行"的问题）。
        let db = Database::new(":memory:").await.unwrap();
        db.upsert_user_process_template(
            "guid-fixed", "old-name", "旧名", "", "测试", "light", "0.1.0",
            "user://test/a.yaml",
        )
        .await
        .unwrap();
        db.upsert_user_process_template(
            "guid-fixed", "new-name", "新名", "", "测试", "light", "0.2.0",
            "user://test/a.yaml",
        )
        .await
        .unwrap();

        let all = db.list_process_templates(None).await.unwrap();
        assert_eq!(all.len(), 1, "同 guid 二次 upsert 不应新增行");
        assert_eq!(all[0].name, "new-name");
    }

    #[tokio::test]
    async fn test_delete_system_process_templates_not_in() {
        // 040 reconcile：只删 guid 不在集合内的系统行，用户行与集合内系统行保留。
        let db = Database::new(":memory:").await.unwrap();
        seed_two_templates(&db).await;
        db.upsert_system_process_template(
            "guid-sys-obsolete", "obsolete", "已下架", "", "测试", "standard", "1.0.0",
            "bundled://processes/test/obsolete.yaml",
        )
        .await
        .unwrap();

        let deleted = db
            .delete_system_process_templates_not_in(&["guid-sys-001".to_string()])
            .await
            .unwrap();
        assert_eq!(deleted, 1);

        let all = db.list_process_templates(None).await.unwrap();
        assert_eq!(all.len(), 2);
        assert!(all.iter().all(|t| t.name != "obsolete"));
        assert!(all.iter().any(|t| !t.is_system), "用户行不应被误删");
    }

    #[tokio::test]
    async fn test_list_process_templates_filter_system_only() {
        let db = Database::new(":memory:").await.unwrap();
        seed_two_templates(&db).await;

        let list = db.list_process_templates(Some(true)).await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "sys-tpl");
        assert!(list[0].is_system);
    }

    #[tokio::test]
    async fn test_list_process_templates_filter_user_only() {
        let db = Database::new(":memory:").await.unwrap();
        seed_two_templates(&db).await;

        let list = db.list_process_templates(Some(false)).await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "user-tpl");
        assert!(!list[0].is_system);
    }

    #[tokio::test]
    async fn test_list_process_templates_none_returns_all() {
        let db = Database::new(":memory:").await.unwrap();
        seed_two_templates(&db).await;

        // None 必须保持旧的全量行为，统计/推荐等旧调用方依赖跨两类聚合。
        let list = db.list_process_templates(None).await.unwrap();
        assert_eq!(list.len(), 2);
    }

    #[tokio::test]
    async fn test_get_process_templates_by_ids_batch() {
        // 列表接口注入工艺名称依赖批量查：一次命中多条、部分命中、空入参三态都要稳。
        let db = Database::new(":memory:").await.unwrap();
        let sys_id = db
            .upsert_system_process_template(
                "guid-sys-001", "sys-tpl", "系统模板", "系统", "测试", "standard", "1.0.0",
                "bundled://processes/test/sys-tpl.yaml",
            )
            .await
            .unwrap();
        let user_id = db
            .upsert_user_process_template(
                "guid-user-001", "user-tpl", "用户模板", "用户", "测试", "light", "0.1.0",
                "user://test/user-tpl.yaml",
            )
            .await
            .unwrap();

        // 命中多条：两条都返回（顺序不保证，按 id 集合断言）。
        let got = db.get_process_templates_by_ids(&[sys_id, user_id]).await.unwrap();
        assert_eq!(got.len(), 2);

        // 部分命中：不存在的 id 静默丢弃，不报错、不补 Null。
        let got = db.get_process_templates_by_ids(&[sys_id, 999_999]).await.unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].id, sys_id);

        // 空入参短路：不落 SQL，直接空 Vec（is_in(空) 在某些后端会生成非法 SQL）。
        let got = db.get_process_templates_by_ids(&[]).await.unwrap();
        assert!(got.is_empty());
    }
}
