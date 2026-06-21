//! Loop Studio + 环节(kind=step)集成测试。 
//! 
//! 覆盖：
//! - v6 migration: todos 加 kind 列, 默认 'item'; 被 loop_steps 引用回填为 'step'
//! - promote_todo_to_step: 把 'item' 升为 'step'
//! - demote_todo_to_item: 'step' → 'item'; 被 loop_steps 引用时必须拒绝
//! - count_loop_steps_using_todo: 被引用次数与 step 增删保持一致
//! - list_steps_with_usage: 包含引用次数; 顺序按 updated_at 倒序
//!
//! 复用 db_feature_supplement_tests 的内存库模式, 不污染开发/生产数据。
//! 内存库走 v1-v7 runner 迁移 (见 db/migration.rs), 自动建出 loops/loop_steps/
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
    db.create_loop(name, "", None, "#722ed1", "loop", None)
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
async fn v6_migration_backfills_step_for_loop_referenced_todos() {
    // 先建 todo + promote 成 step (promote 会同步在 steps 表创建 id=todo_id 的行)，
    // 再建 loop + loop_step 引用该 step_id，模拟「v6 回填后被 loop 引用」的终态。
    let db = setup_db().await;
    let todo_id = create_todo(&db, "被 loop 引用的 todo").await;
    db.promote_to_step(todo_id).await.unwrap();
    let loop_id = create_loop(&db, "测试 loop").await;
    db.create_loop_step(loop_id, "阶段 1", "", todo_id, "sequential", false, None, "skip", true, "next", None, "break", None)
        .await
        .unwrap();

    let todo = db.get_todo(todo_id).await.unwrap().unwrap();
    assert_eq!(todo.kind, "step");
    assert!(
        db.count_loop_steps_using_todo(todo_id).await.unwrap() >= 1,
        "被 step 引用, 引用次数 >= 1"
    );
}

// =====================================================================
// promote / demote
// =====================================================================

#[tokio::test]
async fn promote_item_to_step_changes_kind() {
    let db = setup_db().await;
    let id = create_todo(&db, "待提升").await;
    // 初始 'item'
    assert_eq!(db.get_todo(id).await.unwrap().unwrap().kind, "item");
    db.promote_to_step(id).await.unwrap();
    assert_eq!(db.get_todo(id).await.unwrap().unwrap().kind, "step");
}

#[tokio::test]
async fn demote_step_to_item_succeeds_when_no_loop_refs() {
    let db = setup_db().await;
    let id = create_todo(&db, "未引用环节").await;
    db.promote_to_step(id).await.unwrap();
    db.demote_to_item(id).await.unwrap();
    assert_eq!(db.get_todo(id).await.unwrap().unwrap().kind, "item");
}

#[tokio::test]
async fn demote_step_blocked_when_loop_references_it() {
    let db = setup_db().await;
    let todo_id = create_todo(&db, "被引用的环节").await;
    db.promote_to_step(todo_id).await.unwrap();
    let loop_id = create_loop(&db, "引用 loop").await;
    db.create_loop_step(loop_id, "阶段", "", todo_id, "sequential", false, None, "skip", true, "next", None, "break", None)
        .await
        .unwrap();
    // demote 应该失败 (返回 AppError::BadRequest 或 DbErr)
    let result = db.demote_to_item(todo_id).await;
    assert!(
        result.is_err(),
        "被引用的环节 demote 必须失败, 实际 {:?}",
        result
    );
    // 状态保持 step
    assert_eq!(db.get_todo(todo_id).await.unwrap().unwrap().kind, "step");
}

#[tokio::test]
async fn demote_succeeds_after_stage_deleted() {
    let db = setup_db().await;
    let todo_id = create_todo(&db, "被引用后释放").await;
    db.promote_to_step(todo_id).await.unwrap();
    let loop_id = create_loop(&db, "loop").await;
    let step_id = db
        .create_loop_step(loop_id, "阶段", "", todo_id, "sequential", false, None, "skip", true, "next", None, "break", None)
        .await
        .unwrap()
        .id;
    // 先 demote 失败
    assert!(db.demote_to_item(todo_id).await.is_err());
    // 删 step 后 demote 成功
    db.delete_loop_step(step_id).await.unwrap();
    db.demote_to_item(todo_id).await.unwrap();
    assert_eq!(db.get_todo(todo_id).await.unwrap().unwrap().kind, "item");
}

// =====================================================================
// list_steps / list_steps_with_usage
// =====================================================================

#[tokio::test]
async fn list_steps_excludes_items() {
    let db = setup_db().await;
    let item_id = create_todo(&db, "item 1").await;
    let step_id = create_todo(&db, "step 1").await;
    db.promote_to_step(step_id).await.unwrap();
    let steps = db.list_steps().await.unwrap();
    let ids: Vec<i64> = steps.iter().map(|e| e.id).collect();
    assert!(ids.contains(&step_id));
    assert!(!ids.contains(&item_id), "list_steps 不应包含 item");
}

#[tokio::test]
async fn list_steps_with_usage_includes_count() {
    let db = setup_db().await;
    let e1 = create_todo(&db, "环节 1").await;
    db.promote_to_step(e1).await.unwrap();
    let e2 = create_todo(&db, "环节 2").await;
    db.promote_to_step(e2).await.unwrap();
    // 1 个 step 引用 e2
    let loop_id = create_loop(&db, "loop").await;
    db.create_loop_step(loop_id, "阶段", "", e2, "sequential", false, None, "skip", true, "next", None, "break", None)
        .await
        .unwrap();
    let list = db.list_steps_with_usage().await.unwrap();
    let e1_summary = list.iter().find(|x| x.todo.id == e1).expect("e1 exists");
    let e2_summary = list.iter().find(|x| x.todo.id == e2).expect("e2 exists");
    assert_eq!(e1_summary.used_by_loop_step_count, 0);
    assert_eq!(e2_summary.used_by_loop_step_count, 1);
}

// =====================================================================
// count_loop_steps_using_todo
// =====================================================================

#[tokio::test]
async fn count_loop_steps_reflects_stage_changes() {
    let db = setup_db().await;
    let todo_id = create_todo(&db, "环节").await;
    db.promote_to_step(todo_id).await.unwrap();
    let loop_id = create_loop(&db, "loop").await;

    // 初始 0
    assert_eq!(db.count_loop_steps_using_todo(todo_id).await.unwrap(), 0);

    // 加一个 step → 1
    let s1 = db
        .create_loop_step(loop_id, "s1", "", todo_id, "sequential", false, None, "skip", true, "next", None, "break", None)
        .await
        .unwrap()
        .id;
    assert_eq!(db.count_loop_steps_using_todo(todo_id).await.unwrap(), 1);

    // 加第二个 step → 2
    db.create_loop_step(loop_id, "s2", "", todo_id, "sequential", false, None, "skip", true, "next", None, "break", None)
        .await
        .unwrap();
    assert_eq!(db.count_loop_steps_using_todo(todo_id).await.unwrap(), 2);

    // 删一个 → 1
    db.delete_loop_step(s1).await.unwrap();
    assert_eq!(db.count_loop_steps_using_todo(todo_id).await.unwrap(), 1);
}

// =====================================================================
// update_step 和 delete_step 单元测试
// =====================================================================

/// 测试 update_step 函数：更新标题和颜色
#[tokio::test]
async fn test_update_step_title_and_color() {
    let db = setup_db().await;
    let todo_id = create_todo(&db, "待更新环节").await;
    db.promote_to_step(todo_id).await.unwrap();
    
    // 创建 step 记录
    let step = db.create_step("原始标题", "原始提示", None, None, Some(todo_id), Some("#ff0000"))
        .await.unwrap();
    
    // 更新标题和颜色
    db.update_step(step.id, "新标题", "新提示", Some("claude"), Some("验收标准"), Some("#00ff00"))
        .await.unwrap();
    
    // 验证更新成功
    let updated = db.get_step(step.id).await.unwrap().unwrap();
    assert_eq!(updated.title, "新标题");
    assert_eq!(updated.prompt, "新提示");
    assert_eq!(updated.executor, Some("claude".to_string()));
    assert_eq!(updated.acceptance_criteria, Some("验收标准".to_string()));
    assert_eq!(updated.color, "#00ff00");
}

/// 测试 update_step 函数：不更新颜色（color 为 None）
#[tokio::test]
async fn test_update_step_without_color() {
    let db = setup_db().await;
    let todo_id = create_todo(&db, "不更新颜色").await;
    db.promote_to_step(todo_id).await.unwrap();
    
    let step = db.create_step("测试", "提示", None, None, Some(todo_id), Some("#123456"))
        .await.unwrap();
    
    // 更新但不传颜色
    db.update_step(step.id, "新标题", "新提示", None, None, None)
        .await.unwrap();
    
    // 验证颜色保持不变
    let updated = db.get_step(step.id).await.unwrap().unwrap();
    assert_eq!(updated.title, "新标题");
    assert_eq!(updated.color, "#123456"); // 颜色未变
}

/// 测试 delete_step 函数：正常删除
#[tokio::test]
async fn test_delete_step_success() {
    let db = setup_db().await;
    let todo_id = create_todo(&db, "待删除").await;
    db.promote_to_step(todo_id).await.unwrap();
    
    let step = db.create_step("待删除", "提示", None, None, Some(todo_id), None)
        .await.unwrap();
    let step_id = step.id;
    
    // 删除 step
    db.delete_step(step_id).await.unwrap();
    
    // 验证已删除
    let deleted = db.get_step(step_id).await.unwrap();
    assert!(deleted.is_none(), "step 应该已被删除");
}

/// 测试 delete_step 函数：删除不存在的 step（应该成功，幂等）
#[tokio::test]
async fn test_delete_step_not_found() {
    let db = setup_db().await;
    
    // 删除不存在的 ID，应该成功（幂等操作）
    let result = db.delete_step(99999).await;
    assert!(result.is_ok(), "删除不存在的 step 应该成功");
}

/// 测试 update_step 函数：更新不存在的 step
#[tokio::test]
async fn test_update_step_not_found() {
    let db = setup_db().await;
    
    // 更新不存在的 ID，应该成功但无实际影响
    let result = db.update_step(99999, "标题", "提示", None, None, None).await;
    assert!(result.is_ok(), "更新不存在的 step 应该成功（SQL UPDATE 不影响任何行）");
}
