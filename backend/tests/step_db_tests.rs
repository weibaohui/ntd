//! steps 表 CRUD 单元测试。
//!
//! 覆盖：
//! - create_step: 创建环节，验证默认值和字段赋值
//! - get_step: 按 id 查询，不存在时返回 None
//! - update_step: 更新环节，含 color 和不含 color 两种路径
//! - delete_step: 删除环节，删除后 get_step 返回 None
//!
//! 使用 in-memory SQLite，避免污染开发/生产数据库。
//! 内存库走 migration runner 迁移，自动建出 steps 表。

use ntd::db::Database;

// 共用 setup：in-memory db，migration 自动跑完
async fn setup_db() -> Database {
    Database::new(":memory:").await.unwrap()
}

// 工具函数：创建一个环节，返回完整 Model
async fn create_test_step(db: &Database, title: &str) -> ntd::db::entity::steps::Model {
    db.create_step(
        title,
        "test prompt",
        Some("claude"),
        Some("acceptance criteria"),
        None,
    )
    .await
    .unwrap()
}

// =====================================================================
// create_step 测试
// =====================================================================

#[tokio::test]
async fn test_create_step_stores_all_fields() {
    // 创建环节时传入的所有字段都应正确存储，包括可选字段。
    // source_todo_id 设 None 避免硬编码 todo id 触发 FK；
    // 该字段的下游映射在 create_loop_step 等链路单独测。
    let db = setup_db().await;
    let step = db
        .create_step(
            "测试环节",
            "测试提示词",
            Some("claude"),
            Some("验收标准"),
            None,
        )
        .await
        .unwrap();

    assert_eq!(step.title, "测试环节");
    assert_eq!(step.prompt, "测试提示词");
    assert_eq!(step.executor.as_deref(), Some("claude"));
    assert_eq!(step.acceptance_criteria.as_deref(), Some("验收标准"));
    assert_eq!(step.source_todo_id, None);
}

#[tokio::test]
async fn test_create_step_with_null_optional_fields() {
    // 可选字段传 None 时，应存储为 NULL（Rust 侧为 None）
    let db = setup_db().await;
    let step = db
        .create_step("最简环节", "", None, None, None)
        .await
        .unwrap();

    assert_eq!(step.title, "最简环节");
    assert!(step.executor.is_none());
    assert!(step.acceptance_criteria.is_none());
    assert!(step.source_todo_id.is_none());
}

// =====================================================================
// get_step 测试
// =====================================================================

#[tokio::test]
async fn test_get_step_returns_existing_step() {
    // 创建后可通过 id 查回，字段与创建时一致
    let db = setup_db().await;
    let created = create_test_step(&db, "可查询环节").await;
    let fetched = db.get_step(created.id).await.unwrap().unwrap();

    assert_eq!(fetched.id, created.id);
    assert_eq!(fetched.title, "可查询环节");
}

#[tokio::test]
async fn test_get_step_returns_none_for_nonexistent_id() {
    // 不存在的 id 应返回 Ok(None)，而非 Err
    let db = setup_db().await;
    let result = db.get_step(99999).await.unwrap();
    assert!(result.is_none());
}

// =====================================================================
// update_step 测试
// =====================================================================

#[tokio::test]
async fn test_update_step_basic_fields() {
    // 更新时仅更新基本信息字段，tag 和 color 通过独立 API 管理
    let db = setup_db().await;
    let step = create_test_step(&db, "原标题").await;

    db.update_step(
        step.id,
        "新标题",
        "新提示词",
        Some("gpt4"),
        Some("新验收标准"),
    )
    .await
    .unwrap();

    let updated = db.get_step(step.id).await.unwrap().unwrap();
    assert_eq!(updated.title, "新标题");
    assert_eq!(updated.prompt, "新提示词");
    assert_eq!(updated.executor.as_deref(), Some("gpt4"));
    assert_eq!(updated.acceptance_criteria.as_deref(), Some("新验收标准"));
}

#[tokio::test]
async fn test_update_step_clears_optional_fields() {
    // 更新时将可选字段设为 None，应成功清空对应列
    let db = setup_db().await;
    let step = create_test_step(&db, "有可选字段").await;
    assert!(step.executor.is_some());
    assert!(step.acceptance_criteria.is_some());

    db.update_step(step.id, "标题", "", None, None)
        .await
        .unwrap();

    let updated = db.get_step(step.id).await.unwrap().unwrap();
    assert!(updated.executor.is_none());
    assert!(updated.acceptance_criteria.is_none());
}

// =====================================================================
// delete_step 测试
// =====================================================================

#[tokio::test]
async fn test_delete_step_removes_from_db() {
    // 删除后 get_step 应返回 None
    let db = setup_db().await;
    let step = create_test_step(&db, "待删除").await;
    let step_id = step.id;

    db.delete_step(step_id).await.unwrap();

    let result = db.get_step(step_id).await.unwrap();
    assert!(result.is_none(), "删除后应查不到该环节");
}

#[tokio::test]
async fn test_delete_step_nonexistent_id_succeeds() {
    // 删除不存在的 id 不应报错（SQL DELETE WHERE id=xxx 无匹配行时也返回 0 affected）
    let db = setup_db().await;
    let result = db.delete_step(99999).await;
    assert!(result.is_ok(), "删除不存在的 id 不应报错");
}

// =====================================================================
// list_steps_pure 测试
// =====================================================================

#[tokio::test]
async fn test_list_steps_pure_returns_all_steps_ordered_by_id_desc() {
    // 创建多个环节后，list_steps_pure 应返回全部，且按 id 倒序
    let db = setup_db().await;
    let s1 = create_test_step(&db, "环节A").await;
    let s2 = create_test_step(&db, "环节B").await;
    let s3 = create_test_step(&db, "环节C").await;

    let list = db.list_steps_pure().await.unwrap();
    let ids: Vec<i64> = list.iter().map(|s| s.id).collect();

    assert_eq!(ids.len(), 3);
    // id 倒序：最大的 id 在前
    assert_eq!(ids[0], s3.id);
    assert_eq!(ids[1], s2.id);
    assert_eq!(ids[2], s1.id);
}

// =====================================================================
// count_loop_steps_for_steps 测试
// =====================================================================

#[tokio::test]
async fn test_count_loop_steps_for_steps_empty_input() {
    // 传入空切片应返回空 HashMap，不报错
    let db = setup_db().await;
    let result = db.count_loop_steps_for_steps(&[]).await.unwrap();
    assert!(result.is_empty());
}
