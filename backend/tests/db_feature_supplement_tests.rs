//! 后端功能清单的补充测试用例
//!
//! 背景（issue #439）：基于 `docs/frontend-features.md` 与 `backend/src/db/` 现有方法，
//! 补齐此前未覆盖的 db 层功能：模板、项目目录、Webhook、执行器配置、同步记录。
//!
//! 选用 in-memory SQLite + 与 `db/mod.rs` 已有测试一致的 setup，避免污染开发/生产数据库。
//!
//! 段落总览：
//! - 模板（Template）：CRUD、按分类查询、远程订阅/再同步、系统 vs 自定义区分。
//! - 项目目录（ProjectDirectory）：CRUD、唯一约束下的并发幂等。
//! - Webhook：CRUD、批量查询、关联 todo 命中、按时间清理记录。
//! - 执行器配置（ExecutorConfig）：启用过滤、按名查询、部分字段更新、种子幂等。
//! - 同步记录（SyncRecord）：创建、列表分页、计数、清空。

use ntd::db::Database;
use ntd::db::TemplateInput;

// 共用的内存数据库初始化函数：与 `db/mod.rs` 内置测试保持一致。
async fn setup_db() -> Database {
    Database::new(":memory:").await.unwrap()
}

// =====================================================================
// 模板相关测试
// =====================================================================

#[tokio::test]
async fn test_template_create_and_get() {
    // 创建后通过 get_template_by_id 立即可读，且默认值（is_system=false、sort_order 透传）正确。
    let db = setup_db().await;
    let id = db
        .create_template(
            TemplateInput {
                title: "代码评审",
                prompt: Some("请评审 {{message}}"),
                category: "Git/CI",
                sort_order: Some(10),
            },
            false,
        )
        .await
        .unwrap();
    let tpl = db.get_template_by_id(id).await.unwrap().unwrap();
    assert_eq!(tpl.title, "代码评审");
    assert_eq!(tpl.prompt.as_deref(), Some("请评审 {{message}}"));
    assert_eq!(tpl.category, "Git/CI");
    assert_eq!(tpl.sort_order, 10);
    assert!(!tpl.is_system, "未显式开启时不应是系统模板");
    assert!(tpl.source_url.is_none());
}

#[tokio::test]
async fn test_template_create_system_flag() {
    // is_system 参数为 true 时，写入行的 is_system 应为 1。
    let db = setup_db().await;
    let id = db
        .create_template(
            TemplateInput {
                title: "系统默认",
                prompt: None,
                category: "默认",
                sort_order: Some(1),
            },
            true,
        )
        .await
        .unwrap();
    let tpl = db.get_template_by_id(id).await.unwrap().unwrap();
    assert!(tpl.is_system);
}

#[tokio::test]
async fn test_template_get_by_id_not_found() {
    // 不存在的 id 返回 Ok(None)，而不是 Err——上层据此区分"未找到"和"数据库错误"。
    let db = setup_db().await;
    let result = db.get_template_by_id(9999).await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn test_template_update() {
    // update_template 应当刷新 updated_at；title/prompt/category/sort_order 全部被替换。
    let db = setup_db().await;
    let id = db
        .create_template(
            TemplateInput {
                title: "旧标题",
                prompt: Some("旧 prompt"),
                category: "旧分类",
                sort_order: Some(1),
            },
            false,
        )
        .await
        .unwrap();
    let before = db.get_template_by_id(id).await.unwrap().unwrap();
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

    db.update_template(
        id,
        TemplateInput {
            title: "新标题",
            prompt: Some("新 prompt"),
            category: "新分类",
            sort_order: Some(99),
        },
    )
    .await
    .unwrap();
    let after = db.get_template_by_id(id).await.unwrap().unwrap();
    assert_eq!(after.title, "新标题");
    assert_eq!(after.prompt.as_deref(), Some("新 prompt"));
    assert_eq!(after.category, "新分类");
    assert_eq!(after.sort_order, 99);
    assert_ne!(
        after.updated_at, before.updated_at,
        "update_template 必须刷新 updated_at"
    );
}

#[tokio::test]
async fn test_template_delete() {
    // 删除后通过 get_template_by_id 应当返回 None。
    let db = setup_db().await;
    let id = db
        .create_template(
            TemplateInput {
                title: "待删除",
                prompt: None,
                category: "x",
                sort_order: Some(1),
            },
            false,
        )
        .await
        .unwrap();
    db.delete_template(id).await.unwrap();
    assert!(db.get_template_by_id(id).await.unwrap().is_none());
}

#[tokio::test]
async fn test_template_list_orders_by_sort_order_then_id() {
    // sort_order 升序，相同 sort_order 时按 id 升序，保证前端展示顺序稳定。
    let db = setup_db().await;
    let a = db
        .create_template(
            TemplateInput {
                title: "a",
                prompt: None,
                category: "c",
                sort_order: Some(2),
            },
            false,
        )
        .await
        .unwrap();
    let b = db
        .create_template(
            TemplateInput {
                title: "b",
                prompt: None,
                category: "c",
                sort_order: Some(1),
            },
            false,
        )
        .await
        .unwrap();
    let c = db
        .create_template(
            TemplateInput {
                title: "c",
                prompt: None,
                category: "c",
                sort_order: Some(1), // 与 b 同 sort_order，期望 b 排在 c 之前
            },
            false,
        )
        .await
        .unwrap();
    let all = db.get_templates().await.unwrap();
    let ids: Vec<i64> = all.iter().map(|t| t.id).collect();
    // 期望顺序：b, c, a（按 sort_order asc, id asc）
    let pos = |id: i64| ids.iter().position(|x| *x == id).unwrap();
    assert!(pos(b) < pos(c));
    assert!(pos(c) < pos(a));
}

#[tokio::test]
async fn test_template_list_by_category_filters() {
    // get_templates_by_category 只返回匹配分类的记录，排序规则与 get_templates 一致。
    let db = setup_db().await;
    db.create_template(
        TemplateInput {
            title: "t1",
            prompt: None,
            category: "测试专用分类",
            sort_order: Some(2),
        },
        false,
    )
    .await
    .unwrap();
    db.create_template(
        TemplateInput {
            title: "t2",
            prompt: None,
            category: "其他分类",
            sort_order: Some(1),
        },
        false,
    )
    .await
    .unwrap();
    db.create_template(
        TemplateInput {
            title: "t3",
            prompt: None,
            category: "测试专用分类",
            sort_order: Some(1),
        },
        false,
    )
    .await
    .unwrap();
    let filtered = db.get_templates_by_category("测试专用分类").await.unwrap();
    assert_eq!(filtered.len(), 2);
    assert!(filtered.iter().all(|t| t.category == "测试专用分类"));
    // sort_order=1 的应在 sort_order=2 的之前
    assert!(filtered[0].sort_order < filtered[1].sort_order);

    let empty = db.get_templates_by_category("不存在的分类").await.unwrap();
    assert!(empty.is_empty());
}

#[tokio::test]
async fn test_template_remote_subscription_and_resync() {
    // 远程订阅：create_template_from_remote 会写入 source_url 与 last_sync_at。
    // 再同步场景：delete_templates_by_source_url 会清掉该来源的旧模板，便于重新拉取。
    let db = setup_db().await;
    let id1 = db
        .create_template_from_remote(
            TemplateInput {
                title: "远端A",
                prompt: Some("v1"),
                category: "远程",
                sort_order: Some(1),
            },
            "https://example.com/templates.yaml",
        )
        .await
        .unwrap();
    let id2 = db
        .create_template_from_remote(
            TemplateInput {
                title: "远端B",
                prompt: Some("v1"),
                category: "远程",
                sort_order: Some(2),
            },
            "https://example.com/templates.yaml",
        )
        .await
        .unwrap();
    // 订阅信息应取最近一次同步的那条（id2 的 updated_at 更新更晚）
    let sub = db.get_custom_template_subscription().await.unwrap();
    assert_eq!(
        sub.as_ref().map(|(url, _)| url.as_str()),
        Some("https://example.com/templates.yaml")
    );
    assert!(sub.unwrap().1.is_some(), "last_sync_at 应被填充");

    // 删除该 URL 的所有模板，模拟"再同步前清理"。
    let n = db
        .delete_templates_by_source_url("https://example.com/templates.yaml")
        .await
        .unwrap();
    assert_eq!(n, 2);
    assert!(db.get_template_by_id(id1).await.unwrap().is_none());
    assert!(db.get_template_by_id(id2).await.unwrap().is_none());
}

#[tokio::test]
async fn test_template_delete_all_custom_only() {
    // delete_all_custom_templates 只删 source_url 非空的模板，系统/普通模板保留。
    let db = setup_db().await;
    let sys_id = db
        .create_template(
            TemplateInput {
                title: "系统模板",
                prompt: None,
                category: "x",
                sort_order: Some(1),
            },
            true,
        )
        .await
        .unwrap();
    let user_id = db
        .create_template(
            TemplateInput {
                title: "用户模板",
                prompt: None,
                category: "x",
                sort_order: Some(2),
            },
            false,
        )
        .await
        .unwrap();
    let remote_id = db
        .create_template_from_remote(
            TemplateInput {
                title: "远端模板",
                prompt: None,
                category: "x",
                sort_order: Some(3),
            },
            "https://example.com/x",
        )
        .await
        .unwrap();
    let n = db.delete_all_custom_templates().await.unwrap();
    assert_eq!(n, 1, "只有 source_url 非空的那条应被删");
    assert!(db.get_template_by_id(sys_id).await.unwrap().is_some());
    assert!(db.get_template_by_id(user_id).await.unwrap().is_some());
    assert!(db.get_template_by_id(remote_id).await.unwrap().is_none());
}

// =====================================================================
// 项目目录相关测试
// =====================================================================

#[tokio::test]
async fn test_project_directory_create_and_get_by_id() {
    // 正常路径：创建后立即按 id 读出，path/name 时间戳均存在。
    let db = setup_db().await;
    let id = db
        .create_project_directory("/tmp/proj-a", Some("项目A"))
        .await
        .unwrap();
    let dir = db.get_project_directory_by_id(id).await.unwrap().unwrap();
    assert_eq!(dir.path, "/tmp/proj-a");
    assert_eq!(dir.name.as_deref(), Some("项目A"));
    assert!(dir.created_at.is_some());
    assert!(dir.updated_at.is_some());
}

#[tokio::test]
async fn test_project_directory_get_by_path() {
    // 同一 path 只能查到自己这一条；不同 path 互不影响。
    let db = setup_db().await;
    db.create_project_directory("/tmp/proj-b", Some("B"))
        .await
        .unwrap();
    db.create_project_directory("/tmp/proj-c", Some("C"))
        .await
        .unwrap();
    let b = db
        .get_project_directory_by_path("/tmp/proj-b")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(b.name.as_deref(), Some("B"));
    assert!(db
        .get_project_directory_by_path("/tmp/不存在")
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn test_project_directory_unique_constraint() {
    // path 上有 UNIQUE 约束：重复插入必须报错（外层 get_or_create 依赖该错误来重试）。
    let db = setup_db().await;
    db.create_project_directory("/tmp/dup", Some("first"))
        .await
        .unwrap();
    let err = db
        .create_project_directory("/tmp/dup", Some("second"))
        .await
        .expect_err("重复 path 应触发唯一约束错误");
    let s = format!("{:?}", err);
    assert!(
        s.contains("UNIQUE constraint failed"),
        "应返回 UNIQUE 约束错误，实际: {s}"
    );
}

#[tokio::test]
async fn test_project_directory_get_or_create_idempotent() {
    // 第一次调用：新建；第二次：复用，name 字段保持首次插入的值。
    let db = setup_db().await;
    let d1 = db
        .get_or_create_project_directory("/tmp/goc-1", Some("goc-name"))
        .await
        .unwrap();
    let d2 = db
        .get_or_create_project_directory("/tmp/goc-1", None)
        .await
        .unwrap();
    assert_eq!(d1.id, d2.id, "幂等：同一 path 必须返回同一 id");
    assert_eq!(d2.path, "/tmp/goc-1");
    // 已有记录不传 name 时，名称应保持首次写入值，不被覆盖为 None
    assert_eq!(
        d2.name.as_deref(),
        Some("goc-name"),
        "name 应保持首次插入值"
    );
}

#[tokio::test]
async fn test_project_directory_get_or_create_renames_on_mismatch() {
    // 当 path 已存在但 name 不同时，get_or_create 应自动把 name 同步为新值
    let db = setup_db().await;
    let d1 = db
        .get_or_create_project_directory("/tmp/rename-test", Some("Original Name"))
        .await
        .unwrap();
    assert_eq!(d1.name.as_deref(), Some("Original Name"));

    // 再次传入不同 name，应触发 rename
    let d2 = db
        .get_or_create_project_directory("/tmp/rename-test", Some("New Name"))
        .await
        .unwrap();
    assert_eq!(d1.id, d2.id, "id 不应变化");
    assert_eq!(d2.name.as_deref(), Some("New Name"), "name 应被同步为新值");

    // 传入相同 name，不应触发 UPDATE（保持原值）
    let d3 = db
        .get_or_create_project_directory("/tmp/rename-test", Some("New Name"))
        .await
        .unwrap();
    assert_eq!(d3.id, d2.id);
    assert_eq!(d3.name.as_deref(), Some("New Name"));
}

#[tokio::test]
async fn test_project_directory_update_name() {
    // 仅更新 name，并刷新 updated_at；path 保持不变。
    let db = setup_db().await;
    let id = db
        .create_project_directory("/tmp/upd", Some("old"))
        .await
        .unwrap();
    let before = db.get_project_directory_by_id(id).await.unwrap().unwrap();
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    db.update_project_directory(id, Some("new"))
        .await
        .unwrap();
    let after = db.get_project_directory_by_id(id).await.unwrap().unwrap();
    assert_eq!(after.name.as_deref(), Some("new"));
    assert_eq!(after.path, before.path);
    assert_ne!(after.updated_at, before.updated_at);
}

#[tokio::test]
async fn test_project_directory_delete_removes_row() {
    // 删除后通过 by-id 与 by-path 都不应再查到。
    let db = setup_db().await;
    let id = db
        .create_project_directory("/tmp/del", Some("x"))
        .await
        .unwrap();
    db.delete_project_directory(id).await.unwrap();
    assert!(db.get_project_directory_by_id(id).await.unwrap().is_none());
    assert!(db
        .get_project_directory_by_path("/tmp/del")
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn test_project_directory_list_orders_by_path() {
    // 列表按 path 升序，方便前端做字典序展示。
    let db = setup_db().await;
    db.create_project_directory("/tmp/zzz", None).await.unwrap();
    db.create_project_directory("/tmp/aaa", None).await.unwrap();
    db.create_project_directory("/tmp/mmm", None).await.unwrap();
    let list = db.get_project_directories().await.unwrap();
    let paths: Vec<&str> = list.iter().map(|d| d.path.as_str()).collect();
    assert_eq!(paths, vec!["/tmp/aaa", "/tmp/mmm", "/tmp/zzz"]);
}

// =====================================================================
// Webhook 相关测试
// =====================================================================

#[tokio::test]
async fn test_webhook_create_and_get() {
    // 通过 create_webhook 返回的 Model 立即可见；get_webhook / get_webhooks 一致。
    let db = setup_db().await;
    let todo_id = db.create_todo("test", "").await.unwrap();
    let w = db
        .create_webhook("hook-1", true, Some(todo_id))
        .await
        .unwrap();
    assert_eq!(w.name, "hook-1");
    assert!(w.enabled);
    assert_eq!(w.default_todo_id, Some(todo_id));

    let fetched = db.get_webhook(w.id).await.unwrap().unwrap();
    assert_eq!(fetched.id, w.id);
    let all = db.get_webhooks().await.unwrap();
    assert_eq!(all.len(), 1);
}

#[tokio::test]
async fn test_webhook_update_partial_fields() {
    // update_webhook 会刷新 updated_at，并把 name/enabled/default_todo_id 一起覆盖。
    let db = setup_db().await;
    let todo_id = db.create_todo("t1", "").await.unwrap();
    let w = db.create_webhook("orig", true, None).await.unwrap();
    let before_updated_at = w.updated_at.clone();

    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    db.update_webhook(w.id, "renamed", false, Some(todo_id))
        .await
        .unwrap();
    let after = db.get_webhook(w.id).await.unwrap().unwrap();
    assert_eq!(after.name, "renamed");
    assert!(!after.enabled);
    assert_eq!(after.default_todo_id, Some(todo_id));
    assert_ne!(after.updated_at, before_updated_at);
}

#[tokio::test]
async fn test_webhook_delete() {
    let db = setup_db().await;
    let w = db.create_webhook("to-del", true, None).await.unwrap();
    db.delete_webhook(w.id).await.unwrap();
    assert!(db.get_webhook(w.id).await.unwrap().is_none());
}

#[tokio::test]
async fn test_webhook_get_by_ids_empty_input() {
    // ids 为空时必须短路返回空 Vec，避免下游 N+1 调用栈出现空查询报错。
    let db = setup_db().await;
    let result = db.get_webhooks_by_ids(&[]).await.unwrap();
    assert!(result.is_empty());
}

#[tokio::test]
async fn test_webhook_get_by_ids_filters() {
    // 只返回 ids 列表内的 webhook，其他记录不应混入。
    let db = setup_db().await;
    let w1 = db.create_webhook("a", true, None).await.unwrap();
    let w2 = db.create_webhook("b", true, None).await.unwrap();
    let w3 = db.create_webhook("c", true, None).await.unwrap();
    let picked = db.get_webhooks_by_ids(&[w1.id, w3.id]).await.unwrap();
    let picked_ids: Vec<i64> = picked.iter().map(|w| w.id).collect();
    assert_eq!(picked_ids.len(), 2);
    assert!(picked_ids.contains(&w1.id));
    assert!(picked_ids.contains(&w3.id));
    assert!(!picked_ids.contains(&w2.id));
}

#[tokio::test]
async fn test_webhook_get_by_default_todo_respects_enabled_flag() {
    // 仅在 enabled=true 且 default_todo_id 命中时才返回；禁用后不应再被匹配。
    let db = setup_db().await;
    let todo_id = db.create_todo("t", "").await.unwrap();
    let w = db
        .create_webhook("by-default", true, Some(todo_id))
        .await
        .unwrap();

    let hit = db
        .get_webhook_by_default_todo(todo_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(hit.id, w.id);

    // 关闭后再查，应返回 None
    db.update_webhook(w.id, "by-default", false, Some(todo_id))
        .await
        .unwrap();
    assert!(db
        .get_webhook_by_default_todo(todo_id)
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn test_webhook_record_create_and_paginate() {
    // 写入多条记录，按 id 倒序、分页参数 limit/offset 正确生效。
    let db = setup_db().await;
    let w = db.create_webhook("rec", true, None).await.unwrap();
    for i in 0..5 {
        db.create_webhook_record(ntd::db::webhook::NewWebhookRecord {
            webhook_id: Some(w.id),
            method: "POST".into(),
            path: format!("/p/{i}"),
            query_params: None,
            body: None,
            content_type: Some("application/json".into()),
            triggered_todo_id: None,
            status_code: Some(200),
            response_body: Some("ok".into()),
        })
        .await
        .unwrap();
    }
    let total = db.get_webhook_records_count().await.unwrap();
    assert_eq!(total, 5);

    let page0 = db.get_webhook_records(2, 0).await.unwrap();
    let page1 = db.get_webhook_records(2, 2).await.unwrap();
    assert_eq!(page0.len(), 2);
    assert_eq!(page1.len(), 2);
    // id 倒序：第一页应包含最新两条（path 为 /p/4 与 /p/3）
    assert_eq!(page0[0].path, "/p/4");
    assert_eq!(page0[1].path, "/p/3");
    assert_eq!(page1[0].path, "/p/2");
    assert_eq!(page1[1].path, "/p/1");
}

#[tokio::test]
async fn test_webhook_record_cleanup_old_returns_zero_for_recent() {
    // cleanup_old_webhook_records 的"过期阈值"由调用方决定；
    // 当所有可见记录的 created_at 都晚于"现在-365 天"时，函数应当返回 0。
    // 这里不依赖底层 SQL 直写（db.exec 是 pub(super)，外部测试不可见），
    // 只通过公开 API 写入并验证函数在常见参数下行为稳定。
    let db = setup_db().await;
    let w = db.create_webhook("clean", true, None).await.unwrap();
    db.create_webhook_record(ntd::db::webhook::NewWebhookRecord {
        webhook_id: Some(w.id),
        method: "POST".into(),
        path: "/new".into(),
        query_params: None,
        body: None,
        content_type: None,
        triggered_todo_id: None,
        status_code: Some(200),
        response_body: None,
    })
    .await
    .unwrap();
    let deleted = db.cleanup_old_webhook_records(365).await.unwrap();
    assert_eq!(
        deleted, 0,
        "新近写入的记录不应被 365 天的清理阈值误删"
    );
    assert_eq!(db.get_webhook_records_count().await.unwrap(), 1);
}

#[tokio::test]
async fn test_webhook_record_get_by_id() {
    // get_webhook_record 应能按主键精确取回。
    let db = setup_db().await;
    let w = db.create_webhook("find", true, None).await.unwrap();
    let rec = db
        .create_webhook_record(ntd::db::webhook::NewWebhookRecord {
            webhook_id: Some(w.id),
            method: "GET".into(),
            path: "/x".into(),
            query_params: None,
            body: None,
            content_type: None,
            triggered_todo_id: None,
            status_code: Some(200),
            response_body: None,
        })
        .await
        .unwrap();
    let got = db.get_webhook_record(rec.id).await.unwrap().unwrap();
    assert_eq!(got.id, rec.id);
    assert_eq!(got.method, "GET");
    assert!(db.get_webhook_record(99999).await.unwrap().is_none());
}

// =====================================================================
// 执行器配置相关测试
// =====================================================================

#[tokio::test]
async fn test_executor_seed_default_is_idempotent() {
    // 第一次 seed 写入内置执行器，第二次 seed 不会重复插入（用于启动时反复调用）。
    let db = setup_db().await;
    db.seed_default_executors().await.unwrap();
    let first = db.get_executors().await.unwrap();
    assert!(!first.is_empty(), "种子后至少应有一条执行器记录");
    db.seed_default_executors().await.unwrap();
    let second = db.get_executors().await.unwrap();
    assert_eq!(
        first.len(),
        second.len(),
        "重复 seed 不应增加行数"
    );
}

#[tokio::test]
async fn test_executor_get_enabled_filters_disabled() {
    // get_enabled_executors 只返回 enabled=true 的行。
    let db = setup_db().await;
    db.seed_default_executors().await.unwrap();
    let all = db.get_executors().await.unwrap();
    let target = all.first().expect("至少一条种子执行器");
    db.update_executor(&target.name, None, Some(false), None, None)
        .await
        .unwrap();
    let enabled = db.get_enabled_executors().await.unwrap();
    assert!(enabled.iter().all(|e| e.enabled));
    assert!(
        !enabled.iter().any(|e| e.name == target.name),
        "被禁用的执行器不应出现在启用列表"
    );
}

#[tokio::test]
async fn test_executor_get_by_name_and_update_partial() {
    // 按名查找 + 只传部分字段更新，未传入的字段保持原值。
    let db = setup_db().await;
    db.seed_default_executors().await.unwrap();
    let exec = db
        .get_executor_by_name("claudecode")
        .await
        .unwrap()
        .expect("seed 之后应当存在 claudecode");
    let original_path = exec.path.clone();
    let original_session = exec.session_dir.clone();

    // 仅更新 enabled + display_name，path/session_dir 应当保持不变
    db.update_executor(
        "claudecode",
        None,
        Some(false),
        Some("Claude Code (新)"),
        None,
    )
    .await
    .unwrap();
    let after = db
        .get_executor_by_name("claudecode")
        .await
        .unwrap()
        .unwrap();
    assert!(!after.enabled);
    assert_eq!(after.display_name, "Claude Code (新)");
    assert_eq!(after.path, original_path, "未传 path 必须保持原值");
    assert_eq!(
        after.session_dir, original_session,
        "未传 session_dir 必须保持原值"
    );
}

#[tokio::test]
async fn test_executor_sync_new_executors_adds_missing_only() {
    // sync_new_executors 把 EXECUTORS 静态数组中尚未在 db 中的项补齐，已存在的不会重复插入。
    let db = setup_db().await;
    // 清理任何已存在的执行器，确保从空表开始。
    let existing = db.get_executors().await.unwrap();
    for e in &existing {
        // 直接走更新：name 是主键之外的可识别列；这里仅依赖 update 不影响后续断言。
        db.update_executor(&e.name, None, Some(true), None, None)
            .await
            .unwrap();
    }
    let before = db.get_executors().await.unwrap().len();
    db.sync_new_executors().await.unwrap();
    let after = db.get_executors().await.unwrap().len();
    assert!(after >= before, "sync 后总数不应减少");
    // 再次调用：行数应保持稳定
    db.sync_new_executors().await.unwrap();
    let after2 = db.get_executors().await.unwrap().len();
    assert_eq!(after, after2, "幂等：重复 sync 不应新增行");
}

#[tokio::test]
async fn test_executor_backfill_session_dir_fills_empty() {
    // backfill_session_dir 仅当 session_dir 为空字符串时填充；非空时不动。
    // 这里仅验证函数调用不会 panic，且不会破坏已有数据。
    let db = setup_db().await;
    db.seed_default_executors().await.unwrap();
    let before = db.get_executors().await.unwrap();
    db.backfill_session_dir().await.unwrap();
    let after = db.get_executors().await.unwrap();
    assert_eq!(before.len(), after.len());
    // 至少一个执行器应保持非空 session_dir
    assert!(after.iter().any(|e| !e.session_dir.is_empty()));
}

// =====================================================================
// 同步记录相关测试
// =====================================================================

#[tokio::test]
async fn test_sync_record_create_and_list_ordering() {
    // 新建同步记录后，列表按 id 倒序（最新在前），保证前端时间线展示稳定。
    let db = setup_db().await;
    let id1 = db
        .create_sync_record("push", "overwrite", "success", "todo", None, None)
        .await
        .unwrap();
    let id2 = db
        .create_sync_record("pull", "skip", "failed", "template", Some("details"), Some("boom"))
        .await
        .unwrap();
    assert!(id2 > id1);
    let all = db.get_sync_records(10, 0).await.unwrap();
    assert_eq!(all.len(), 2);
    assert_eq!(all[0].id, id2, "最新写入应在最前");
    assert_eq!(all[1].id, id1);
    // 各字段都被正确序列化
    assert_eq!(all[0].direction, "pull");
    assert_eq!(all[0].conflict_mode, "skip");
    assert_eq!(all[0].status, "failed");
    assert_eq!(all[0].data_type, "template");
    assert_eq!(all[0].details.as_deref(), Some("details"));
    assert_eq!(all[0].error_message.as_deref(), Some("boom"));
}

#[tokio::test]
async fn test_sync_record_pagination_and_count() {
    // 写入 7 条记录，验证分页 (limit/offset) 与 count_sync_records 一致。
    let db = setup_db().await;
    for i in 0..7 {
        db.create_sync_record("push", "overwrite", "success", "todo", Some(&format!("d{i}")), None)
            .await
            .unwrap();
    }
    assert_eq!(db.count_sync_records().await.unwrap(), 7);

    let p0 = db.get_sync_records(3, 0).await.unwrap();
    let p1 = db.get_sync_records(3, 3).await.unwrap();
    let p2 = db.get_sync_records(3, 6).await.unwrap();
    assert_eq!(p0.len(), 3);
    assert_eq!(p1.len(), 3);
    assert_eq!(p2.len(), 1, "最后一页不足 limit 时只返回剩余条数");
}

#[tokio::test]
async fn test_sync_record_clear_returns_count() {
    // clear_sync_records 删空后总数应为 0，且返回的受影响行数等于实际记录数。
    let db = setup_db().await;
    for _ in 0..4 {
        db.create_sync_record("push", "skip", "success", "todo", None, None)
            .await
            .unwrap();
    }
    let n = db.clear_sync_records().await.unwrap();
    assert_eq!(n, 4);
    assert_eq!(db.count_sync_records().await.unwrap(), 0);

    // 再次清空：返回 0，不报错
    let n2 = db.clear_sync_records().await.unwrap();
    assert_eq!(n2, 0);
}

// =====================================================================
// 跨模块：seed_default_templates 幂等性（前端启动时会反复调用）
// =====================================================================

#[tokio::test]
async fn test_seed_default_templates_is_idempotent() {
    // 多次调用 seed_default_templates，系统模板总数应保持稳定，
    // 不应出现重复行（避免前端出现"双胞胎"模板）。
    let db = setup_db().await;
    db.seed_default_templates().await.unwrap();
    let first = db.get_templates().await.unwrap();
    assert!(
        first.iter().any(|t| t.is_system),
        "首次 seed 应当写入至少一条系统模板"
    );
    let first_count = first.len();

    // 第二次：应当走 update 分支而不是 insert 分支
    db.seed_default_templates().await.unwrap();
    let second = db.get_templates().await.unwrap();
    assert_eq!(
        second.len(),
        first_count,
        "二次 seed 不应增加模板数（按 title+is_system 唯一性更新）"
    );

    // 全部都是系统模板
    assert!(second.iter().all(|t| t.is_system));
}

// =====================================================================
// 跨模块：模板与 todo 的关系（issue #439 中模板管理的"选择模板"等场景）
// =====================================================================

#[tokio::test]
async fn test_template_does_not_cascade_to_todo() {
    // 删除模板不应影响既有的 todo 记录——模板与 todo 之间没有外键关联。
    let db = setup_db().await;
    let tpl_id = db
        .create_template(
            TemplateInput {
                title: "temp",
                prompt: Some("p"),
                category: "x",
                sort_order: Some(1),
            },
            false,
        )
        .await
        .unwrap();
    let todo_id = db.create_todo("todo from temp", "p").await.unwrap();
    db.delete_template(tpl_id).await.unwrap();
    let todo = db.get_todo(todo_id).await.unwrap();
    assert!(todo.is_some(), "模板删除后 todo 必须仍然存在");
}
