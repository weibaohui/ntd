//! 数据库迁移 V94：把「项目目录」实体重命名为「工作空间」。
//!
//! 背景：工作空间功能从「项目目录」演化而来（见 docs/design/104-工作空间命名统一重构-设计.md），
//! 表名 project_directories 与现行 workspace 命名并存。本迁移把表/索引/触发器/列名统一为 workspace。
//! 注意：V94 之前的旧迁移（v1..v93）引用的是当时的表名 project_directories，属不可变历史，
//! 本迁移在它们之后执行，SQL 里必须使用旧表名做 RENAME 的源表名。

use async_trait::async_trait;

use super::super::Database;
use super::{table_exists, table_has_column, Migration};

pub(super) struct V94RenameProjectDirectoriesToWorkspaces;

#[async_trait]
impl Migration for V94RenameProjectDirectoriesToWorkspaces {
    fn version(&self) -> i64 {
        94
    }
    fn name(&self) -> &'static str {
        "rename_project_directories_to_workspaces"
    }

    async fn up(&self, db: &Database) -> Result<(), sea_orm::DbErr> {
        // 幂等守卫（防半途残留态）：迁移 runner 里 up() 与写 schema_version 不在同一事务，
        // 若 RENAME 已成功但进程在记录版本前崩溃，重启后 V94 重跑会因为源表不存在而
        // 永久失败，库打不开（与 V87 BUG-009 自愈同一思路）。
        // 完成判定必须覆盖表改名 AND 列改名两个维度：up() 内各 DDL 独立自动提交，
        // 崩溃可能落在「表已改名、列未改名」的中间态——只判表会把这种库误判为已完成，
        // 跳过后 feishu_project_bindings.project_dir_id 永不改名，而 main 上代码已全面
        // 改用 workspace_id 列名，飞书绑定查询会静默失败。列守卫放在跳过条件里而非
        // 放在后面单独兜底，正是为了让中间态继续走完剩余步骤而非被整体跳过。
        if table_exists(db, "workspaces").await?
            && !table_exists(db, "project_directories").await?
            && !table_has_column(db, "feishu_project_bindings", "project_dir_id").await?
        {
            return Ok(());
        }

        // 先删旧触发器与旧名索引：SQLite 没有 RENAME INDEX / RENAME TRIGGER，
        // 必须显式 drop 后以新名重建，否则库里会残留带 project_directories 名字的对象。
        // IF EXISTS：中间态重跑时这些对象可能已随表改名而不存在。
        db.exec("DROP TRIGGER IF EXISTS set_project_directories_created_at_utc")
            .await?;
        db.exec("DROP INDEX IF EXISTS idx_project_directories_path").await?;

        // 表改名。SQLite ≥3.25 会自动把其它表 CREATE 语句里
        // REFERENCES project_directories(id) 改写为 REFERENCES workspaces(id)，
        // 无需手工重建外键表（漂移测试会校验这一点）。
        // 守卫：中间态（表已改名、列未改名）重跑时源表已不存在，直接 RENAME 会报
        // no such table 使库打不开——这正是守卫要自愈的场景，跳过即可。
        if table_exists(db, "project_directories").await? {
            db.exec("ALTER TABLE project_directories RENAME TO workspaces")
                .await?;
        }

        // 以新名重建 path 唯一索引与 created_at UTC 触发器，保持旧库既有行为不变。
        // IF NOT EXISTS 与守卫配合：任一中间态崩溃重跑时，已完成的步骤都能安全通过。
        db.exec("CREATE INDEX IF NOT EXISTS idx_workspaces_path ON workspaces(path)")
            .await?;
        db.exec(
            "CREATE TRIGGER IF NOT EXISTS set_workspaces_created_at_utc AFTER INSERT ON workspaces
             BEGIN
                 UPDATE workspaces SET created_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now', 'utc') WHERE rowid = new.rowid;
             END",
        )
        .await?;

        // 飞书绑定表的 project_dir_id 列名一并统一为 workspace_id（API 可见字段），
        // 与系统内其它 workspace_id 列（agent_bots/todos/blackboards 等）对齐。
        // 列存在性守卫：这是 up() 的最后一步，完整跑完后列必然不存在，
        // 中间态重跑时则据此补完剩余改名。
        if table_has_column(db, "feishu_project_bindings", "project_dir_id").await? {
            db.exec("ALTER TABLE feishu_project_bindings RENAME COLUMN project_dir_id TO workspace_id")
                .await?;
        }
        Ok(())
    }
}

#[cfg(test)]
// 测试断言用 expect/panic 直接失败即可，与 v57/v87 等历史迁移测试的豁免口径一致。
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use super::super::super::Database;
    use super::super::Migration;

    use super::V94RenameProjectDirectoriesToWorkspaces;

    /// 直接查询某表的某列值（单行字符串），测试断言用。
    /// None 表示行不存在或值为 NULL，由调用方区分断言语义。
    async fn query_text(db: &Database, sql: &str) -> Option<String> {
        use sea_orm::ConnectionTrait;
        db.conn
            .query_one(sea_orm::Statement::from_string(
                sea_orm::DbBackend::Sqlite,
                sql.to_string(),
            ))
            .await
            .expect("query must succeed")?
            .try_get_by_index::<String>(0)
            .ok()
    }

    /// 检查表是否存在（复用 mod.rs 的判定口径，避免测试里手写 SQL 漂移）。
    async fn has_table(db: &Database, table: &str) -> bool {
        crate::db::migration::table_exists(db, table)
            .await
            .expect("table_exists must succeed")
    }

    /// 完整老库升级回归：v93 库（表名/列名均为旧命名 + 存量数据）跑到 V94，
    /// 断言表/列/索引/触发器全部换新名、数据一行不丢、外键引用被 SQLite 自动改写。
    /// 这是 104 命名统一重构的数据安全底线——RENAME 天然保数据，但飞书列改名与
    /// 外键改写行为必须有显式测试兜底（漂移测试只比对 DDL，不覆盖数据与 FK 行为）。
    #[tokio::test]
    async fn test_v94_old_db_upgrade_preserves_data_and_renames_all() {
        let db = Database::connect_without_migrations(":memory:")
            .await
            .expect(":memory: db must open");
        // v93 是 V94 的直接前置版本：跑到 v93 即得到「升级前夜」的老库状态
        db.run_migrations_with(93).await.expect("build v93 state");

        // 在旧命名下铺真实数据：一行 workspace + 一行飞书绑定（project_dir_id 列）。
        // feishu_project_bindings 的 FK 链是 bot_id → agent_bots → workspace，
        // 需先建 bot 父行；FK 由连接强制开启（PRAGMA foreign_keys=ON），造数违规会直接失败。
        db.exec(
            "INSERT INTO project_directories (id, path, name) VALUES (9, '/legacy-ws', 'legacy空间')",
        )
        .await
        .expect("insert legacy workspace");
        db.exec(
            "INSERT INTO agent_bots (bot_type, bot_name, app_id, app_secret, workspace_id)
             VALUES ('feishu', 'legacy-bot', 'app-x', 'sec-x', 9)",
        )
        .await
        .expect("insert legacy bot");
        db.exec(
            "INSERT INTO feishu_project_bindings
             (bot_id, chat_id, chat_type, project_dir_id, todo_id, created_at, updated_at)
             VALUES (1, 'oc_legacy', 'group', 9, 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
        )
        .await
        .expect("insert legacy binding");

        // 执行 V94
        V94RenameProjectDirectoriesToWorkspaces
            .up(&db)
            .await
            .expect("V94 must succeed");

        // 表已换新名，旧名消失
        assert!(has_table(&db, "workspaces").await, "表应已改名为 workspaces");
        assert!(
            !has_table(&db, "project_directories").await,
            "旧表名 project_directories 应不存在"
        );
        // workspace 存量数据完好
        assert_eq!(
            query_text(&db, "SELECT name FROM workspaces WHERE id = 9").await,
            Some("legacy空间".to_string()),
            "存量 workspace 行必须原样保留"
        );
        // 飞书绑定列已改名且数据完好。
        // 注意用 CAST(... AS TEXT) 而非直接读：try_get_by_index::<String> 对 INTEGER 列
        // 解码会失败返回 None（SQLite 动态类型 + sqlx 严格按声明类型解码），并非数据丢失。
        assert_eq!(
            query_text(
                &db,
                "SELECT CAST(workspace_id AS TEXT) FROM feishu_project_bindings WHERE chat_id = 'oc_legacy'"
            )
            .await,
            Some("9".to_string()),
            "project_dir_id 应改名为 workspace_id 且值不变"
        );
        // 外键引用被 SQLite ≥3.25 自动改写：blackboards 的 FK 应指向 workspaces
        let blackboards_ddl = query_text(
            &db,
            "SELECT sql FROM sqlite_master WHERE name = 'blackboards'",
        )
        .await
        .expect("blackboards 表必须存在且 DDL 非空");
        assert!(
            blackboards_ddl.contains(r#"REFERENCES "workspaces"(id)"#),
            "外键表 DDL 中的引用应被自动改写为 workspaces(id)，实际: {blackboards_ddl}"
        );
        // 新名索引/触发器就位（COUNT 返回 INTEGER，同样要 CAST 成 TEXT 再断言）
        assert_eq!(
            query_text(
                &db,
                "SELECT CAST(COUNT(*) AS TEXT) FROM sqlite_master WHERE type = 'index' AND name = 'idx_workspaces_path'"
            )
            .await,
            Some("1".to_string()),
            "idx_workspaces_path 索引应以新名重建"
        );
        assert_eq!(
            query_text(
                &db,
                "SELECT CAST(COUNT(*) AS TEXT) FROM sqlite_master WHERE type = 'trigger' AND name = 'set_workspaces_created_at_utc'"
            )
            .await,
            Some("1".to_string()),
            "set_workspaces_created_at_utc 触发器应以新名重建"
        );
        // FK 行为验证（RENAME 后约束语义不变）：blackboards 对 workspaces 是
        // ON DELETE CASCADE——先在 workspace 9 下挂一条黑板，删除 workspace 后
        // 黑板应被级联清掉，证明外键在新表名下真实生效（而非仅 DDL 文本改写）。
        // 必须钉在同一条连接上执行 DELETE：Database::exec 走连接池（max=10），每条
        // 语句可能落在不同连接；v1-v93 增量链里有迁移以 PRAGMA foreign_keys=OFF/ON
        // 重建表，而 PRAGMA 只对当时那条连接生效——池中部分连接可能停留在 FK=OFF，
        // DELETE 落上去 CASCADE 不触发，断言就会随机失败（本测试初期踩过的坑）。
        // 钉法：取池底层连接，先 PRAGMA foreign_keys=ON 再 INSERT/DELETE，
        // 三条语句确定同连接同 FK 状态。PRAGMA 必须在事务外执行（SQLite 对事务内
        // 的该 PRAGMA 视为 no-op），所以不用 begin() 事务、而是直接持有裸连接。
        // SeaORM 提供的 get_sqlite_connection_pool 直接取底层 sqlx 池，
        // acquire 一条连接钉住（仅测试内使用，不污染生产代码）。
        let pool = db.conn.get_sqlite_connection_pool();
        let mut conn = pool
            .acquire()
            .await
            .expect("acquire pinned connection");
        exec_on_pinned(&mut conn, "PRAGMA foreign_keys = ON").await;
        exec_on_pinned(&mut conn, "INSERT INTO blackboards (workspace_id, content) VALUES (9, 'c')")
            .await;
        exec_on_pinned(&mut conn, "DELETE FROM workspaces WHERE id = 9").await;
        drop(conn);
        assert_eq!(
            query_text(
                &db,
                "SELECT CAST(COUNT(*) AS TEXT) FROM blackboards WHERE workspace_id = 9"
            )
            .await,
            Some("0".to_string()),
            "blackboards 子行应被 ON DELETE CASCADE 级联删除，FK 在新表名下仍生效"
        );
    }

    /// 半途残留态自愈：RENAME 已完成但版本未记录（进程在 record_migration 前崩溃），
    /// 重跑 V94 必须直接跳过而非报「no such table」导致库永久打不开。
    #[tokio::test]
    async fn test_v94_is_idempotent_after_interrupted_rename() {
        let db = Database::new(":memory:")
            .await
            .expect(":memory: db must open");
        // bootstrap 已是最终 schema（workspaces），模拟「表已改名但 v94 未记录」：
        // 直接重跑 up()，守卫应识别已完成状态并跳过
        V94RenameProjectDirectoriesToWorkspaces
            .up(&db)
            .await
            .expect("残留态下重跑 V94 应被守卫跳过而非报错");
        assert!(has_table(&db, "workspaces").await);
    }

    /// 中间态自愈：崩溃落在「表已改名、列未改名」之间（up() 各 DDL 独立自动提交），
    /// 重跑 V94 必须补完剩余列改名，而非被表级守卫误判为已完成整体跳过。
    /// 这是表+列双维守卫的回归用例——只判表会让 feishu 绑定列永远停留旧名。
    #[tokio::test]
    async fn test_v94_resumes_column_rename_after_interrupted_table_rename() {
        let db = Database::connect_without_migrations(":memory:")
            .await
            .expect(":memory: db must open");
        db.run_migrations_with(93).await.expect("build v93 state");

        // 手工执行到表改名为止，模拟进程在此之后、列改名之前崩溃：
        // drop 旧触发器/索引 → RENAME 表（中间态：列还是 project_dir_id）。
        db.exec("DROP TRIGGER IF EXISTS set_project_directories_created_at_utc")
            .await
            .expect("drop old trigger");
        db.exec("DROP INDEX IF EXISTS idx_project_directories_path")
            .await
            .expect("drop old index");
        db.exec("ALTER TABLE project_directories RENAME TO workspaces")
            .await
            .expect("simulate interrupted table rename");

        // 此刻处于中间态：表已换新名，但飞书绑定列还是旧名
        assert!(has_table(&db, "workspaces").await);
        assert!(
            table_has_column_for_test(&db, "feishu_project_bindings", "project_dir_id").await,
            "前置校验：中间态下列应仍为 project_dir_id"
        );

        // 重跑 V94：守卫不误判，补完列改名与索引/触发器重建
        V94RenameProjectDirectoriesToWorkspaces
            .up(&db)
            .await
            .expect("中间态重跑 V94 应补完剩余步骤而非报错");

        // 列最终被改名为 workspace_id，旧列名消失
        assert!(
            !table_has_column_for_test(&db, "feishu_project_bindings", "project_dir_id").await,
            "中间态重跑后旧列名 project_dir_id 应不存在"
        );
        assert!(
            table_has_column_for_test(&db, "feishu_project_bindings", "workspace_id").await,
            "中间态重跑后列应已改名为 workspace_id"
        );
        // 索引/触发器也应在新名下就位（重跑补完了全部剩余步骤）
        assert_eq!(
            query_text(
                &db,
                "SELECT CAST(COUNT(*) AS TEXT) FROM sqlite_master WHERE type = 'index' AND name = 'idx_workspaces_path'"
            )
            .await,
            Some("1".to_string()),
            "idx_workspaces_path 索引应在重跑后重建"
        );
    }

    /// 在钉住的连接上执行单条 SQL（测试用，失败即 panic）。
    /// 借用 sqlx 原生 execute：sea-orm 的 TransactionTrait 在此处不便使用，
    /// 因 PRAGMA foreign_keys 在事务内是 no-op，必须裸连接执行。
    async fn exec_on_pinned(conn: &mut sqlx::SqliteConnection, sql: &str) {
        use sqlx::Executor;
        conn.execute(sql)
            .await
            .unwrap_or_else(|e| panic!("pinned exec must succeed: {sql} -> {e}"));
    }

    /// 测试内直接访问 mod.rs 的 table_has_column（与 has_table 同样复用判定口径）。
    async fn table_has_column_for_test(db: &Database, table: &str, column: &str) -> bool {
        crate::db::migration::table_has_column(db, table, column)
            .await
            .expect("table_has_column must succeed")
    }
}

