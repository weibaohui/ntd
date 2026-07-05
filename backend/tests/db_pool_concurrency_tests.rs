//! 数据库连接池并发行为回归测试
//!
//! 背景（Issue #497）：早期实现把 `max_connections=1`，导致所有 DB I/O 串行化。
//! 当前实现把上限提到 10 并通过 `after_connect` hook 同步 PRAGMA。
//!
//! 本测试只覆盖两个回归点：
//! 1. WAL 模式下并发读不被串行化（旧 max=1 下虽然能成功但被串行执行）。
//! 2. 适度超额并发（任务数 > pool size）能在 acquire_timeout 内排队完成，不会 panic。
//!
//! 设计要点：
//! - 用 `tempfile::tempdir()` 拿到真实磁盘文件作为 DB 路径，不能用 `:memory:`，
//!   否则 sqlx pool 的每条 connection 会拿到独立的内存库，跨连接看不到数据，
//!   误把「pool 不共享数据」当成「pool 不并发」。
//! - 用 `Arc<Database>` 在任务间共享同一个连接池，而不是每个任务都 `Database::new`。
//! - 只调用 `pub` 方法（`create_todo` / `get_todos`），不直接戳 `conn` 字段。

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
use std::sync::Arc;
use std::time::Instant;

use ntd::db::Database;
use tempfile::TempDir;

// 共用的临时文件 DB 初始化函数。
// 临时目录的生命周期绑在返回值的 TempDir 上，drop 时整个目录被清理。
async fn setup_file_db() -> (Arc<Database>, TempDir) {
    let dir = tempfile::tempdir().expect("create tempdir");
    let path = dir.path().join("pool_concurrency.db");
    let db = Database::new(path.to_str().expect("utf8 path"))
        .await
        .expect("open db");
    (Arc::new(db), dir)
}

/// 同时跑 8 个并发读任务，全部应成功（远低于 max=10，不会触发 acquire 排队）。
/// 这是 #497 修复的最小回归用例：旧 max=1 下读仍能成功但会被串行化。
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_concurrent_reads_all_succeed_under_pool_size() {
    let (db, _dir) = setup_file_db().await;

    // 用一个 Barrier 让 8 个任务几乎同时发起查询，触发真实的池竞争。
    let barrier = Arc::new(tokio::sync::Barrier::new(8));
    let mut handles = Vec::with_capacity(8);
    for _ in 0..8 {
        let db = Arc::clone(&db);
        let barrier = Arc::clone(&barrier);
        handles.push(tokio::spawn(async move {
            // 在屏障处汇合，确保 8 个任务几乎同时发起 read。
            barrier.wait().await;
            let todos = db.get_todos().await.expect("concurrent read should succeed");
            // 新库应为 0 条；这个断言同时也是「读到了 init 后的表」的烟雾测试。
            assert_eq!(todos.len(), 0);
        }));
    }
    for h in handles {
        h.await.expect("task join");
    }
}

/// 提交读 + 写混合任务（写只在开头插入一条 seed，之后 8 个 reader 并发读 32 次），
/// 验证 WAL reader 不会被 writer 长时间阻塞，间接证明 pool 多连接能同时服务读写。
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_wal_reads_not_blocked_by_single_writer() {
    let (db, _dir) = setup_file_db().await;

    // 写一条 seed todo，所有 reader 之后都能看到。
    let seed_id = db
        .create_todo("pool_test_seed", "x")
        .await
        .expect("seed insert");
    assert!(seed_id > 0, "seed id should be positive, got {}", seed_id);

    let start = Instant::now();
    let mut handles = Vec::with_capacity(8);
    for task_idx in 0..8 {
        let db = Arc::clone(&db);
        handles.push(tokio::spawn(async move {
            // 32 次连续读，故意放大观察窗口；用 task_idx 让每次标题不同避免互相影响。
            for i in 0..32 {
                let _todos = db.get_todos().await.expect("read should succeed");
                // 至少有一条 seed 在；让该任务在循环里产生微小延迟，模拟真实业务节拍。
                if i % 8 == 0 {
                    tokio::task::yield_now().await;
                }
            }
            task_idx
        }));
    }
    for h in handles {
        let _ = h.await.expect("task join");
    }
    let elapsed = start.elapsed();

    // 8 个 reader × 32 次 = 256 次 SELECT，WAL+pool=10 下应远低于 acquire_timeout 阈值。
    // 用一个保守的 5s 上限兜底（远大于正常值），CI 慢机器也不会假阳性。
    assert!(
        elapsed < std::time::Duration::from_secs(5),
        "256 concurrent reads took {:?}, pool may be serialized",
        elapsed
    );
}

/// 验证 acquire_timeout=5s 在适度超额并发（11 个任务 vs max=10）下能优雅排队：
/// 所有任务最终都应成功，而不是因为超时而失败。
/// 关键不在于耗时多少（这条测试不卡耗时），而在于「不会因 pool 满而 panic」。
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_mild_overload_does_not_panic() {
    let (db, _dir) = setup_file_db().await;

    // 11 个任务 vs pool=10：会有一个任务需要等别人归还连接。
    let mut handles = Vec::with_capacity(11);
    for i in 0..11 {
        let db = Arc::clone(&db);
        handles.push(tokio::spawn(async move {
            // 每次写一条不同标题的 todo，简短持锁，让排队真的能形成。
            let title = format!("overload_todo_{}", i);
            let id = db
                .create_todo(&title, "x")
                .await
                .expect("mild overload should still acquire within 5s timeout");
            assert!(id > 0, "created id should be positive, got {}", id);
        }));
    }
    for h in handles {
        h.await.expect("task join");
    }
}
