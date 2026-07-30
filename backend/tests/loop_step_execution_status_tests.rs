//! `mark_step_execution_started` 回归测试。
//!
//! 背景：人工审批步骤复用已 completed 的 todo 时，LoopRunner 创建 step_execution
//! 即写入 `pending_approval`（见 loop_runner 4e）。旧版 `mark_step_execution_started`
//! 无条件把状态改回 `running`，冲掉了初始状态，导致步骤卡在 running、
//! 前端人工审批按钮不出现。修复后：`pending_approval` 状态必须被保留，
//! 其余状态仍正常流转为 `running`。

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]

use ntd::db::Database;

/// 内存库初始化，与 db_core_coverage_tests 保持一致。
async fn setup_db() -> Database {
    Database::new(":memory:").await.unwrap()
}

/// 构造最小链路：todo → loop → step → loop_execution，
/// 再以指定初始状态创建 step_execution，返回其 id 与所属 loop_execution_id。
async fn build_step_execution(db: &Database, initial_status: &str) -> (i64, i64) {
    let todo_id = db.create_todo("approval-test-todo", "").await.unwrap();
    let lp = db
        .create_loop("approval-loop", "", None, None, false, "", None, None, None, "[]")
        .await
        .unwrap();
    let step = db
        .create_loop_step(
            lp.id, "step_1", "", todo_id, "sequence", false, None, "skip",
            true, "next", None, "fail", None, "human",
        )
        .await
        .unwrap();
    let lp_exec = db
        .create_loop_execution(lp.id, None, "manual", "{}", 1)
        .await
        .unwrap();
    let step_exec = db
        .create_loop_step_execution(lp_exec.id, step.id, todo_id, initial_status, 0, None, "skip")
        .await
        .unwrap();
    (step_exec.id, lp_exec.id)
}

/// 读取指定 loop_execution 下第一个 step_execution 的当前状态。
async fn read_step_status(db: &Database, loop_execution_id: i64) -> (String, bool) {
    let execs = db.list_loop_step_executions(loop_execution_id).await.unwrap();
    let first = execs.into_iter().next().expect("step execution must exist");
    (first.status, first.started_at.is_some())
}

#[tokio::test]
async fn test_mark_step_execution_started_preserves_pending_approval() {
    let db = setup_db().await;
    let (step_exec_id, lp_exec_id) = build_step_execution(&db, "pending_approval").await;

    db.mark_step_execution_started(step_exec_id).await.unwrap();

    // 关键断言：pending_approval 不得被冲掉；started_at 仍要记录，供耗时统计使用。
    let (status, has_started_at) = read_step_status(&db, lp_exec_id).await;
    assert_eq!(status, "pending_approval");
    assert!(has_started_at);
}

#[tokio::test]
async fn test_mark_step_execution_started_sets_running_for_normal_step() {
    let db = setup_db().await;
    let (step_exec_id, lp_exec_id) = build_step_execution(&db, "pending").await;

    db.mark_step_execution_started(step_exec_id).await.unwrap();

    // 普通步骤维持原语义：pending → running。
    let (status, has_started_at) = read_step_status(&db, lp_exec_id).await;
    assert_eq!(status, "running");
    assert!(has_started_at);
}
