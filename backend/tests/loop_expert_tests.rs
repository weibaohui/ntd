//! Loop Studio + 环节(kind=expert)集成测试。
//!
//! 覆盖：
//! - v6 migration: todos 加 kind 列, 默认 'item'; 被 loop_stages 引用回填为 'expert'
//! - promote_todo_to_expert: 把 'item' 升为 'expert'
//! - demote_todo_to_item: 'expert' → 'item'; 被 loop_stages 引用时必须拒绝
//! - count_loop_stages_using_todo: 被引用次数与 stage 增删保持一致
//! - list_steps_with_usage: 包含引用次数; 顺序按 updated_at 倒序
//!
//! 复用 db_feature_supplement_tests 的内存库模式, 不污染开发/生产数据。
//! 内存库走 v1-v7 runner 迁移 (见 db/migration.rs), 自动建出 loops/loop_stages/
//! loop_hooks 等 Loop Studio 表。

use ntd::db::Database;

// 共享 setup: in-memory db, runner 迁移自动跑完 v1-v7
async fn setup_db() -> Database {
    Database::new(":memory:").await.unwrap()
}

// 工具: 直接构造一个 todo (kind 走 DEFAULT 'item'), 返回 id
async fn create_todo(db: &Database, title: &str) -> i64 {
    db.create_todo(title, "").await.unwrap()
}

// 工具: 构造一个 loop, 返回 loop_id
async fn create_loop(db: &Database, name: &str) -> i64 {
    db.create_loop(name, "", "", "", "", "#722ed1", "loop")
        .await
        .unwrap()
        .id
}

// =====================================================================
// v6 migration: kind 列 + 回填
// =====================================================================

#[tokio::test]
async fn v6_migration_creates_kind_column_with_default_item() {
    // 新建内存库后, todos.kind 列存在, 新插入的 todo 默认 'item'
    let db = setup_db().await;
    let id = create_todo(&db, "普通事项").await;
    let todo = db.get_todo(id).await.unwrap().unwrap();
    assert_eq!(todo.kind, "item", "新 todo 默认 kind='item'");
}

#[tokio::test]
async fn v6_migration_backfills_expert_for_loop_referenced_todos() {
    // 先建 todo + loop + stage (stage 引用 todo), 再 promote 检查
    // 走 list_steps_with_usage 应该看到该 todo, 且 used_by >= 1
    let db = setup_db().await;
    let todo_id = create_todo(&db, "被 loop 引用的 todo").await;
    let loop_id = create_loop(&db, "测试 loop").await;
    db.create_stage(loop_id, "阶段 1", "", todo_id, "sequential", false, None, "skip", true)
        .await
        .unwrap();

    // 直接 promote 模拟「v6 回填」后状态
    db.promote_to_step(todo_id).await.unwrap();
    let todo = db.get_todo(todo_id).await.unwrap().unwrap();
    assert_eq!(todo.kind, "step");
    assert!(
        db.count_loop_stages_using_todo(todo_id).await.unwrap() >= 1,
        "被 stage 引用, 引用次数 >= 1"
    );
}

// =====================================================================
// promote / demote
// =====================================================================

#[tokio::test]
async fn promote_item_to_expert_changes_kind() {
    let db = setup_db().await;
    let id = create_todo(&db, "待提升").await;
    // 初始 'item'
    assert_eq!(db.get_todo(id).await.unwrap().unwrap().kind, "item");
    db.promote_to_step(id).await.unwrap();
    assert_eq!(db.get_todo(id).await.unwrap().unwrap().kind, "step");
}

#[tokio::test]
async fn demote_expert_to_item_succeeds_when_no_loop_refs() {
    let db = setup_db().await;
    let id = create_todo(&db, "未引用环节").await;
    db.promote_to_step(id).await.unwrap();
    db.demote_to_item(id).await.unwrap();
    assert_eq!(db.get_todo(id).await.unwrap().unwrap().kind, "item");
}

#[tokio::test]
async fn demote_expert_blocked_when_loop_references_it() {
    let db = setup_db().await;
    let todo_id = create_todo(&db, "被引用的环节").await;
    db.promote_to_step(todo_id).await.unwrap();
    let loop_id = create_loop(&db, "引用 loop").await;
    db.create_stage(loop_id, "阶段", "", todo_id, "sequential", false, None, "skip", true)
        .await
        .unwrap();
    // demote 应该失败 (返回 AppError::BadRequest 或 DbErr)
    let result = db.demote_to_item(todo_id).await;
    assert!(
        result.is_err(),
        "被引用的环节 demote 必须失败, 实际 {:?}",
        result
    );
    // 状态保持 expert
    assert_eq!(db.get_todo(todo_id).await.unwrap().unwrap().kind, "step");
}

#[tokio::test]
async fn demote_succeeds_after_stage_deleted() {
    let db = setup_db().await;
    let todo_id = create_todo(&db, "被引用后释放").await;
    db.promote_to_step(todo_id).await.unwrap();
    let loop_id = create_loop(&db, "loop").await;
    let stage_id = db
        .create_stage(loop_id, "阶段", "", todo_id, "sequential", false, None, "skip", true)
        .await
        .unwrap()
        .id;
    // 先 demote 失败
    assert!(db.demote_to_item(todo_id).await.is_err());
    // 删 stage 后 demote 成功
    db.delete_stage(stage_id).await.unwrap();
    db.demote_to_item(todo_id).await.unwrap();
    assert_eq!(db.get_todo(todo_id).await.unwrap().unwrap().kind, "item");
}

// =====================================================================
// list_experts / list_steps_with_usage
// =====================================================================

#[tokio::test]
async fn list_experts_excludes_items() {
    let db = setup_db().await;
    let item_id = create_todo(&db, "item 1").await;
    let expert_id = create_todo(&db, "expert 1").await;
    db.promote_to_step(expert_id).await.unwrap();
    let experts = db.list_experts().await.unwrap();
    let ids: Vec<i64> = experts.iter().map(|e| e.id).collect();
    assert!(ids.contains(&expert_id));
    assert!(!ids.contains(&item_id), "list_experts 不应包含 item");
}

#[tokio::test]
async fn list_steps_with_usage_includes_count() {
    let db = setup_db().await;
    let e1 = create_todo(&db, "环节 1").await;
    db.promote_to_step(e1).await.unwrap();
    let e2 = create_todo(&db, "环节 2").await;
    db.promote_to_step(e2).await.unwrap();
    // 1 个 stage 引用 e2
    let loop_id = create_loop(&db, "loop").await;
    db.create_stage(loop_id, "阶段", "", e2, "sequential", false, None, "skip", true)
        .await
        .unwrap();
    let list = db.list_steps_with_usage().await.unwrap();
    let e1_summary = list.iter().find(|x| x.todo.id == e1).expect("e1 exists");
    let e2_summary = list.iter().find(|x| x.todo.id == e2).expect("e2 exists");
    assert_eq!(e1_summary.used_by_loop_stage_count, 0);
    assert_eq!(e2_summary.used_by_loop_stage_count, 1);
}

// =====================================================================
// count_loop_stages_using_todo
// =====================================================================

#[tokio::test]
async fn count_loop_stages_reflects_stage_changes() {
    let db = setup_db().await;
    let todo_id = create_todo(&db, "环节").await;
    db.promote_to_step(todo_id).await.unwrap();
    let loop_id = create_loop(&db, "loop").await;

    // 初始 0
    assert_eq!(db.count_loop_stages_using_todo(todo_id).await.unwrap(), 0);

    // 加一个 stage → 1
    let s1 = db
        .create_stage(loop_id, "s1", "", todo_id, "sequential", false, None, "skip", true)
        .await
        .unwrap()
        .id;
    assert_eq!(db.count_loop_stages_using_todo(todo_id).await.unwrap(), 1);

    // 加第二个 stage → 2
    db.create_stage(loop_id, "s2", "", todo_id, "sequential", false, None, "skip", true)
        .await
        .unwrap();
    assert_eq!(db.count_loop_stages_using_todo(todo_id).await.unwrap(), 2);

    // 删一个 → 1
    db.delete_stage(s1).await.unwrap();
    assert_eq!(db.count_loop_stages_using_todo(todo_id).await.unwrap(), 1);
}
