//! 重新生成合并版 schema：`src/db/migration/consolidated_schema.rs`。
//!
//! 改动任何迁移后，若 `cargo test test_consolidated_schema_matches_incremental` 失败，
//! 说明合并 schema 与增量迁移脱节，运行本生成器重写该文件即可（然后重新跑漂移测试）。
//!
//! 运行方式：`cargo test --test dbg_gen_schema -- --ignored --nocapture`
//! （默认 `#[ignore]`，避免常规测试误覆盖源码文件。）

// 生成器脚本走简化写法，放宽 clippy（与其它测试文件一致）
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::print_stdout, clippy::print_stderr)]

use std::fs;
use sea_orm::{ConnectionTrait, DbBackend, Statement};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "显式重生成合并 schema 用，避免常规测试覆盖源码"]
async fn gen_consolidated_schema() {
    // 走 bootstrap 建出的全新库 dump DDL；漂移测试保证它与增量迁移结果一致。
    let db = ntd::db::Database::new(":memory:")
        .await
        .expect("memory db must open");
    let rows = db
        ._conn_raw()
        .query_all(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT type, name, sql FROM sqlite_master
             WHERE type IN ('table','index')
               AND name NOT LIKE 'sqlite_%'
               AND name NOT IN ('schema_version', '_loop_phase_executions_new')
             ORDER BY type, name".to_string(),
        ))
        .await
        .expect("dump");

    // 表在前、索引在后（索引依赖表存在）
    let tables: Vec<String> = rows.iter()
        .filter(|r| r.try_get_by::<String, _>("type").unwrap_or_default() == "table")
        .map(|r| r.try_get_by::<String, _>("sql").unwrap_or_default())
        .collect();
    let indexes: Vec<String> = rows.iter()
        .filter(|r| r.try_get_by::<String, _>("type").unwrap_or_default() == "index")
        .map(|r| r.try_get_by::<String, _>("sql").unwrap_or_default())
        .collect();

    let mut out = String::new();
    // 版本号不写死：生成时刻的最新版本即真相，避免头部注释随迁移增长而漂移失真
    out.push_str("//! 合并版最终 schema（全部迁移应用后的状态），供全新库一次性建表（bootstrap）。\n");
    out.push_str("//! 自动生成：全新库跑完增量迁移后 dump sqlite_master。\n");
    out.push_str("//! 改动任何迁移后需重新生成：`cargo test --test dbg_gen_schema -- --ignored`\n\n");
    out.push_str("pub const CONSOLIDATED_SCHEMA: &[&str] = &[\n");
    for t in &tables {
        out.push_str("    r#\"");
        out.push_str(t.trim());
        out.push_str("\"#,\n");
    }
    for i in &indexes {
        out.push_str("    r#\"");
        out.push_str(i.trim());
        out.push_str("\"#,\n");
    }
    out.push_str("];\n");

    // 路径必须相对 CARGO_MANIFEST_DIR（backend/ 目录）推导：原硬编码绝对路径只在
    // 原作者机器存在，其他人跑生成器会直接 panic（本分支实测）。
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/db/migration/consolidated_schema.rs"
    );
    fs::write(path, &out).expect("write consolidated_schema.rs");
    eprintln!("GEN OK: {} tables, {} indexes, {} bytes -> {path}", tables.len(), indexes.len(), out.len());
}
