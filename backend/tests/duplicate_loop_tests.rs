//! `duplicate_loop` 回归测试：审计 #1。
//!
//! 旧实现把源 loop 的 `success_goto_step_id` / `fail_goto_step_id` 原样写入副本，
//! 但新 step 拿到的是全新自增 id，导致副本的 goto 指向源 loop 的步骤（悬空/错误）。
//! 修复后应把 goto 从「源 step id」重映射到「新 step id」。
//!
//! 测试策略：在 source loop 里构造 step_a.success_goto = step_b（源内引用），
//! duplicate 后断言副本 step_a 的 goto 指向副本 step_b（新 id），而非源 step_b（旧 id）。

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]

use ntd::db::Database;

/// 内存库初始化，与 db_core_coverage_tests 保持一致。
async fn setup_db() -> Database {
    Database::new(":memory:").await.unwrap()
}

/// 构造一个 source loop，含两个 step：step_b（goto 无）与 step_a（success_goto 指向 step_b）。
/// 返回 (loop_id, step_a_id, step_b_id)。
async fn build_loop_with_goto(db: &Database) -> (i64, i64, i64) {
    // create_todo 返回新 todo 的 id（i64）。
    let todo_id = db.create_todo("goto-test-todo", "").await.unwrap();
    // workspace_id 传 None：duplicate_loop 不依赖 workspace，只需 loop 本身存在。
    let lp = db
        .create_loop(
            "src-loop",
            "",
            None,
            None,
            false,
            "",
            None,
            None,
            None,
            "[]",
        )
        .await
        .unwrap();

    // 先建 step_b（goto 为空），拿到其 id 后再建 step_a 的 goto 指向它。
    let step_b = db
        .create_loop_step(
            lp.id,
            "step_b",
            "",
            todo_id,
            "sequence",
            false,
            None,
            "skip",
            true,
            "next",
            None,
            "fail",
            None,
            "none",
        )
        .await
        .unwrap();
    let step_a = db
        .create_loop_step(
            lp.id,
            "step_a",
            "",
            todo_id,
            "sequence",
            false,
            None,
            "skip",
            true,
            "goto",
            Some(step_b.id), // success_goto 指向源 loop 的 step_b
            "fail",
            None,
            "none",
        )
        .await
        .unwrap();
    (lp.id, step_a.id, step_b.id)
}

#[tokio::test]
async fn test_duplicate_loop_remaps_success_goto_to_new_step_id() {
    let db = setup_db().await;
    // src_step_a 仅用于对比副本 id 不同，不直接参与断言（goto 断言用 src_step_b）。
    let (src_loop_id, _src_step_a, src_step_b) = build_loop_with_goto(&db).await;

    // 复制前断言源 loop 的 setup 正确：step_a 的 goto 指向源 step_b。
    let src_steps = db.list_loop_steps_by_loop(src_loop_id).await.unwrap();
    let src_a = src_steps.iter().find(|s| s.name == "step_a").unwrap();
    assert_eq!(src_a.success_goto_step_id, Some(src_step_b));

    // 复制 loop（修复后应重映射 goto）。
    let copied = db.duplicate_loop(src_loop_id).await.unwrap().unwrap();

    // 取副本的 steps，断言 step_a 的 goto 指向副本内的 step_b（新 id），而非源 step_b（旧 id）。
    let new_steps = db.list_loop_steps_by_loop(copied.id).await.unwrap();
    let new_a = new_steps.iter().find(|s| s.name == "step_a").unwrap();
    let new_b = new_steps.iter().find(|s| s.name == "step_b").unwrap();

    // 关键断言：goto 目标是副本 step_b 的新 id，且不等于源 step_b 的旧 id。
    assert_eq!(new_a.success_goto_step_id, Some(new_b.id));
    assert_ne!(new_a.success_goto_step_id, Some(src_step_b));

    // 副本 step 自身 id 也应与源不同（自增）。
    assert_ne!(new_a.id, src_a.id);
    assert_ne!(new_b.id, src_step_b);
}
