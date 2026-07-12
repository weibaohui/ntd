use sea_orm::{ActiveModelTrait, ActiveValue, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder};

use crate::adapters::EXECUTORS;
use crate::db::entity::executors;
use crate::db::Database;
use crate::models::ExecutorConfig;

fn map_executor(m: executors::Model) -> ExecutorConfig {
    ExecutorConfig {
        id: m.id,
        name: m.name,
        path: m.path,
        enabled: m.enabled,
        display_name: m.display_name,
        session_dir: m.session_dir,
        is_default: m.is_default,
        created_at: m.created_at,
        updated_at: m.updated_at,
    }
}

impl Database {
    pub async fn get_executors(&self) -> Result<Vec<ExecutorConfig>, sea_orm::DbErr> {
        let models = executors::Entity::find()
            .order_by_asc(executors::Column::Id)
            .all(&self.conn)
            .await?;
        Ok(models.into_iter().map(map_executor).collect())
    }

    pub async fn get_enabled_executors(&self) -> Result<Vec<ExecutorConfig>, sea_orm::DbErr> {
        let models = executors::Entity::find()
            .filter(executors::Column::Enabled.eq(true))
            .order_by_asc(executors::Column::Id)
            .all(&self.conn)
            .await?;
        Ok(models.into_iter().map(map_executor).collect())
    }

    pub async fn get_executor_by_name(&self, name: &str) -> Result<Option<ExecutorConfig>, sea_orm::DbErr> {
        let model = executors::Entity::find()
            .filter(executors::Column::Name.eq(name))
            .one(&self.conn)
            .await?;
        Ok(model.map(map_executor))
    }

    pub async fn update_executor(
        &self,
        name: &str,
        path: Option<&str>,
        enabled: Option<bool>,
        display_name: Option<&str>,
        session_dir: Option<&str>,
    ) -> Result<(), sea_orm::DbErr> {
        let model = executors::Entity::find()
            .filter(executors::Column::Name.eq(name))
            .one(&self.conn)
            .await?;
        // 幽灵 id 走静默 no-op 是历史契约；显式 warn 让上游 handler 拼错 id 时
        // 能在日志里看到提示,避免配置错误被静默吞掉。
        let Some(m) = model else {
            tracing::warn!(
                executor_name = %name,
                "update_executor called with unknown executor name; no-op"
            );
            return Ok(());
        };
        let now = crate::models::utc_timestamp();
        let mut am: executors::ActiveModel = m.into();
        if let Some(p) = path {
            am.path = ActiveValue::Set(p.to_string());
        }
        if let Some(e) = enabled {
            am.enabled = ActiveValue::Set(e);
        }
        if let Some(d) = display_name {
            am.display_name = ActiveValue::Set(d.to_string());
        }
        if let Some(sd) = session_dir {
            am.session_dir = ActiveValue::Set(sd.to_string());
        }
        am.updated_at = ActiveValue::Set(Some(now));
        am.update(&self.conn).await?;
        Ok(())
    }

    /// Migrate executor paths from config.yaml into database.
    /// Only runs when the executors table is empty.
    pub async fn migrate_from_config(
        &self,
        cfg_executors: &crate::config::ExecutorPaths,
    ) -> Result<(), sea_orm::DbErr> {
        let count = executors::Entity::find().count(&self.conn).await?;
        // 首次迁移语义: 表非空时不再写 DB,避免覆盖用户通过 UI 改过的 path。
        // 显式 warn 让用户改 config.yaml 重启后能在日志里看到"配置被忽略"的提示,
        // 否则升级二进制路径的场景会悄无声息失效。
        if count > 0 {
            tracing::warn!(
                configured_executors = cfg_executors.paths.len(),
                "executors table non-empty; skipping config.yaml migration (update via UI instead)"
            );
            return Ok(());
        }

        let now = crate::models::utc_timestamp();

        for (i, exec) in EXECUTORS.iter().enumerate() {
            // 第一个执行器设为默认（通常是 claudecode），
            // 保持与历史行为一致：默认执行器就是列表第一个。
            let is_default = i == 0;
            // Try primary name first, then aliases (for backward compatibility with legacy config keys)
            let path = cfg_executors.paths.get(exec.name)
                .or_else(|| {
                    exec.aliases.iter()
                        .find_map(|alias| cfg_executors.paths.get(*alias))
                })
                .map(|s| s.as_str())
                .unwrap_or(exec.default_path);
            let am = executors::ActiveModel {
                name: ActiveValue::Set(exec.name.to_string()),
                path: ActiveValue::Set(path.to_string()),
                enabled: ActiveValue::Set(true),
                display_name: ActiveValue::Set(exec.display_name.to_string()),
                session_dir: ActiveValue::Set(exec.session_dir.to_string()),
                is_default: ActiveValue::Set(is_default),
                created_at: ActiveValue::Set(Some(now.clone())),
                updated_at: ActiveValue::Set(Some(now.clone())),
                ..Default::default()
            };
            am.insert(&self.conn).await?;
        }

        tracing::info!("Migrated executor paths from config.yaml to database");
        Ok(())
    }

    /// Seed default executors if table is empty (fresh install).
    /// 全新安装时，第一个执行器（claudecode）会被设为默认执行器。
    pub async fn seed_default_executors(&self) -> Result<(), sea_orm::DbErr> {
        let count = executors::Entity::find().count(&self.conn).await?;
        if count > 0 {
            return Ok(());
        }

        let now = crate::models::utc_timestamp();
        for (i, exec) in EXECUTORS.iter().enumerate() {
            // 第一个执行器设为默认（通常是 claudecode），
            // 保持与历史行为一致：默认执行器就是列表第一个。
            let is_default = i == 0;
            let am = executors::ActiveModel {
                name: ActiveValue::Set(exec.name.to_string()),
                path: ActiveValue::Set(exec.default_path.to_string()),
                enabled: ActiveValue::Set(true),
                display_name: ActiveValue::Set(exec.display_name.to_string()),
                session_dir: ActiveValue::Set(exec.session_dir.to_string()),
                is_default: ActiveValue::Set(is_default),
                created_at: ActiveValue::Set(Some(now.clone())),
                updated_at: ActiveValue::Set(Some(now.clone())),
                ..Default::default()
            };
            am.insert(&self.conn).await?;
        }

        tracing::info!("Seeded default executors into database");
        Ok(())
    }

    /// Backfill session_dir for existing executors that have empty session_dir.
    pub async fn backfill_session_dir(&self) -> Result<(), sea_orm::DbErr> {
        let models = executors::Entity::find().all(&self.conn).await?;
        for m in models {
            if m.session_dir.is_empty() {
                if let Some(exec) = EXECUTORS.iter().find(|e| e.name == m.name) {
                    if !exec.session_dir.is_empty() {
                        let mut am: executors::ActiveModel = m.into();
                        am.session_dir = ActiveValue::Set(exec.session_dir.to_string());
                        am.update(&self.conn).await?;
                    }
                }
            }
        }
        Ok(())
    }

    /// Sync new executors from EXECUTORS static array into database.
    /// Adds any executors that exist in EXECUTORS but not in the database.
    pub async fn sync_new_executors(&self) -> Result<(), sea_orm::DbErr> {
        let existing = executors::Entity::find().all(&self.conn).await?;
        let existing_names: Vec<&str> = existing.iter().map(|m| m.name.as_str()).collect();
        let now = crate::models::utc_timestamp();

        // 增：代码里有但 DB 里没有的 executor → 插入
        for exec in EXECUTORS {
            if !existing_names.contains(&exec.name) {
                let am = executors::ActiveModel {
                    name: ActiveValue::Set(exec.name.to_string()),
                    path: ActiveValue::Set(exec.default_path.to_string()),
                    enabled: ActiveValue::Set(true),
                    display_name: ActiveValue::Set(exec.display_name.to_string()),
                    session_dir: ActiveValue::Set(exec.session_dir.to_string()),
                    created_at: ActiveValue::Set(Some(now.clone())),
                    updated_at: ActiveValue::Set(Some(now.clone())),
                    ..Default::default()
                };
                am.insert(&self.conn).await?;
                tracing::info!("Added new executor '{}' to database", exec.name);
            }
        }

        // 删：DB 里有但代码里没有的 enabled executor → 自动禁用
        let code_names: Vec<&str> = EXECUTORS.iter().map(|e| e.name).collect();
        for model in &existing {
            if model.enabled && !code_names.contains(&model.name.as_str()) {
                let mut am: executors::ActiveModel = model.clone().into();
                am.enabled = ActiveValue::Set(false);
                am.updated_at = ActiveValue::Set(Some(now.clone()));
                am.update(&self.conn).await?;
                tracing::info!(
                    "Disabled removed executor '{}' (no longer supported)",
                    model.name
                );
            }
        }

        Ok(())
    }

    /// 获取系统默认执行器。
    /// 如果没有标记为默认的执行器，返回 None；
    /// 调用方应自行决定回退策略（如使用 claudecode 常量）。
    pub async fn get_default_executor(&self) -> Result<Option<ExecutorConfig>, sea_orm::DbErr> {
        let model = executors::Entity::find()
            .filter(executors::Column::IsDefault.eq(true))
            .one(&self.conn)
            .await?;
        Ok(model.map(map_executor))
    }

    /// 获取默认执行器的名称，便捷方法。
    /// 优先从数据库读取 is_default=true 的执行器，
    /// 如果没有则回退到 `DEFAULT_EXECUTOR` 常量（claudecode）。
    /// 用于创建 todo 等需要默认执行器名的场景。
    pub async fn get_default_executor_name(&self) -> Result<String, sea_orm::DbErr> {
        let default = self.get_default_executor().await?;
        Ok(default
            .map(|e| e.name)
            .unwrap_or_else(|| crate::adapters::DEFAULT_EXECUTOR.to_string()))
    }

    /// 设置指定执行器为系统默认执行器。
    /// 会先清除所有执行器的默认标记，再将指定执行器设为默认，
    /// 确保同一时间只有一个默认执行器。
    /// 返回更新后的执行器配置。
    pub async fn set_default_executor(&self, name: &str) -> Result<Option<ExecutorConfig>, sea_orm::DbErr> {
        // 先确认目标执行器存在，不存在则返回 None
        let target = executors::Entity::find()
            .filter(executors::Column::Name.eq(name))
            .one(&self.conn)
            .await?;
        let Some(target) = target else {
            return Ok(None);
        };

        let now = crate::models::utc_timestamp();

        // 清除所有执行器的默认标记
        executors::Entity::update_many()
            .col_expr(executors::Column::IsDefault, sea_orm::sea_query::Expr::value(false))
            .exec(&self.conn)
            .await?;

        // 将目标执行器设为默认
        let mut am: executors::ActiveModel = target.clone().into();
        am.is_default = ActiveValue::Set(true);
        am.updated_at = ActiveValue::Set(Some(now));
        let updated = am.update(&self.conn).await?;

        Ok(Some(map_executor(updated)))
    }
}
