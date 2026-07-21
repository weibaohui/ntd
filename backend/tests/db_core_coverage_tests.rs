//! 后端核心 DB 模块的补充单元测试（Issue #681: 提升后端测试用例覆盖度）。
//!
//! 选型依据：
//! - 这些模块（tag / sync_record / agent_bot / executor_config 的大部分函数）
//!   在主仓里只有 db/mod.rs 的"烟雾测试"覆盖度，但每个函数都直接对应核心功能：
//!   tag 用于 Todo 分类，sync_record 用于云同步历史，agent_bot 用于飞书机器人接入，
//!   executor_config 是执行器注册表的真实来源。
//! - 在 issue #681 的语境下，"有意义的测试"= 覆盖 CRUD 主路径 + 边界（空表、
//!   重复插入、级联删除、字段级 update、过滤查询） + 排序/分页契约。
//!
//! 段落总览：
//! - tag 模块：创建/查询/删除/重命名、find_tag_by_name、todo-tag 多对多、
//!   set_todo_tags 事务语义。
//! - sync_record 模块：分页、总数、清空、按 id 倒序的稳定性。
//! - agent_bot 模块：CRUD、get_agent_bot、update_agent_bot_config、
//!   feishu 类型自动生成 p2p/group response config 的级联效果。
//! - executor_config 模块：get_enabled_executors 过滤、update_executor 字段级、
//!   sync_new_executors 增/禁用分支。

// 测试代码允许 unwrap/expect/panic 等写法以简化断言逻辑，统一放宽以下 clippy 检查
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
use ntd::config::ExecutorPaths;
use ntd::db::Database;
use ntd::db::entity::{executors, feishu_response_config, tags, todo_tags};
use sea_orm::{ActiveValue, ColumnTrait, EntityTrait, QueryFilter};
use std::collections::{HashMap, HashSet};

// 共用的内存数据库初始化函数：与 db/mod.rs 内置测试保持一致。
async fn setup_db() -> Database {
    Database::new(":memory:").await.unwrap()
}

// =====================================================================
// Tag 模块测试
// =====================================================================

#[cfg(test)]
mod tag_tests {
    use super::*;

    /// 验证 create_tag → get_tags → find_tag_by_name → delete_tag 主链路：
    ///   1) get_tags 返回按 name 升序；
    ///   2) find_tag_by_name 找到对应 id；
    ///   3) delete_tag 后无法再查到。
    #[tokio::test]
    async fn test_tag_crud_main_path() {
        let db = setup_db().await;
        // 故意插入乱序名字，验证按 name 升序排列的契约。
        let id_zeta = db.create_tag("zeta", "#ff0000").await.unwrap();
        let id_alpha = db.create_tag("alpha", "#00ff00").await.unwrap();

        let tags = db.get_tags().await.unwrap();
        assert_eq!(tags.len(), 2);
        assert_eq!(tags[0].name, "alpha", "应按 name 升序");
        assert_eq!(tags[0].id, id_alpha);
        assert_eq!(tags[1].name, "zeta");
        assert_eq!(tags[1].id, id_zeta);

        // find_tag_by_name 精确匹配
        assert_eq!(db.find_tag_by_name("alpha").await.unwrap(), Some(id_alpha));
        assert_eq!(db.find_tag_by_name("ghost").await.unwrap(), None);

        // 删除后不应再查到
        db.delete_tag(id_alpha).await.unwrap();
        assert_eq!(db.find_tag_by_name("alpha").await.unwrap(), None);
        assert_eq!(db.get_tags().await.unwrap().len(), 1);
    }

    /// tag 与 todo 的多对多关系：
    ///   add_todo_tag 重复调用是幂等的；set_todo_tags 会先清空再重建集合。
    /// 选用先 add 再 set 的组合：这是前端表单"打标签 → 整体提交"的真实场景。
    /// 通过 db._conn_raw() + todo_tags Entity 直查关联表来真正断言事务级语义,
    /// 而非仅看 add_todo_tag / set_todo_tags 的返回 Ok。
    #[tokio::test]
    async fn test_todo_tag_associations_and_set_replaces() {
        let db = setup_db().await;
        let todo_id = db.create_todo("t1", "").await.unwrap();
        let tag_a = db.create_tag("bug", "#f00").await.unwrap();
        let tag_b = db.create_tag("feature", "#0f0").await.unwrap();
        let tag_c = db.create_tag("chore", "#00f").await.unwrap();

        // 查 todo_tags 表的辅助函数：避免每处都重复写 Entity::find().filter
        let fetch_linked = |todo_id: i64| {
            let db = &db;
            async move {
                todo_tags::Entity::find()
                    .filter(todo_tags::Column::TodoId.eq(todo_id))
                    .all(db._conn_raw())
                    .await
                    .unwrap()
            }
        };

        // 重复 add 同一个 (todo,tag) 必须幂等 —— 不能让主键冲突冒泡成 DbErr
        db.add_todo_tag(todo_id, tag_a).await.unwrap();
        db.add_todo_tag(todo_id, tag_a).await.unwrap();
        db.add_todo_tag(todo_id, tag_b).await.unwrap();
        let links = fetch_linked(todo_id).await;
        assert_eq!(links.len(), 2, "add_todo_tag 幂等后应有 2 条关联");
        let linked_ids: HashSet<i64> = links.iter().map(|l| l.tag_id).collect();
        assert_eq!(linked_ids, HashSet::from([tag_a, tag_b]));

        // set_todo_tags 把 todo 的关联整体替换为 {tag_a, tag_c}
        //   tag_b 应当消失；tag_a 保留；tag_c 新增。
        db.set_todo_tags(todo_id, &[tag_a, tag_c]).await.unwrap();
        let links = fetch_linked(todo_id).await;
        assert_eq!(links.len(), 2, "set_todo_tags 替换后应有 2 条关联");
        let linked_ids: HashSet<i64> = links.iter().map(|l| l.tag_id).collect();
        assert_eq!(
            linked_ids,
            HashSet::from([tag_a, tag_c]),
            "tag_b 应当消失,tag_c 应当新增"
        );

        // set_todo_tags([]) 验证清空分支 —— 事务级语义,直接查表
        db.set_todo_tags(todo_id, &[]).await.unwrap();
        let links = fetch_linked(todo_id).await;
        assert!(
            links.is_empty(),
            "set_todo_tags(empty) 必须清空关联 —— 前端'取消所有标签'依赖这条"
        );

        // 再次 add 应当成功 —— 没有残留旧关联就不会撞 UNIQUE
        db.add_todo_tag(todo_id, tag_b).await.unwrap();
        let links = fetch_linked(todo_id).await;
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].tag_id, tag_b);
    }

    /// get_tag_backups 是云同步 export 用的接口；颜色为空时回退到空串,
    /// 不能 panic。云同步客户端依赖这个字段非 null 才能序列化为合法 JSON。
    #[tokio::test]
    async fn test_get_tag_backups_handles_null_color() {
        let db = setup_db().await;
        // 直插一行 color=NULL 的 tag,模拟历史脏数据：
        // create_tag 永远会包成 Some(...),所以这里走 entity 层用 ActiveValue::Set(None)
        // 才能真正覆盖 get_tag_backups 的 unwrap_or_default 分支。
        // 注意列有 DEFAULT '#1890ff',用 ActiveValue::NotSet 会被默认值填上而不是 NULL。
        tags::Entity::insert(tags::ActiveModel {
            name: ActiveValue::Set("legacy".to_string()),
            color: ActiveValue::Set(None),
            ..Default::default()
        })
        .exec(db._conn_raw())
        .await
        .unwrap();

        let backups = db.get_tag_backups().await.unwrap();
        assert_eq!(backups.len(), 1);
        assert_eq!(backups[0].name, "legacy");
        // color 是 String,不应当是 None —— 即使 DB 里是 NULL 也得给空串兜底
        assert_eq!(backups[0].color, "");
    }

    /// get_tags 的 NULL-color 分支：与 get_tag_backups 共用同一段
    /// `m.color.unwrap_or_default()` 的 NULL 兜底逻辑（见 backend/src/db/tag.rs），
    /// 单独覆盖 get_tags 路径,避免某天只改其中一处忘了另一处。
    /// PR #682 评审 CRITICAL #2 指出：原 test_tag_crud_main_path 走 create_tag
    /// 永远走 Some(color) 分支,NULL 路径到不了;改用 ActiveValue::Set(None) 直插
    /// 才是真正覆盖 NULL 兜底的写法。
    #[tokio::test]
    async fn test_get_tags_handles_null_color() {
        let db = setup_db().await;
        // 正常色 + NULL 色 各插一条,确认两条都被读到、且 NULL 那条返回空串而非 None/panic
        db.create_tag("painted", "#aabbcc").await.unwrap();
        tags::Entity::insert(tags::ActiveModel {
            name: ActiveValue::Set("legacy".to_string()),
            color: ActiveValue::Set(None),
            ..Default::default()
        })
        .exec(db._conn_raw())
        .await
        .unwrap();

        let tags_out = db.get_tags().await.unwrap();
        assert_eq!(tags_out.len(), 2, "正常色与 NULL 色 tag 都应被读到");

        let legacy = tags_out
            .iter()
            .find(|t| t.name == "legacy")
            .expect("NULL 色 legacy tag 必须在返回列表里");
        // 与 get_tag_backups 行为一致：NULL → 空串,而非 None / panic
        assert_eq!(legacy.color, "", "get_tags 必须给 NULL color 兜底成空串");

        let painted = tags_out
            .iter()
            .find(|t| t.name == "painted")
            .expect("正常色 painted tag 必须在返回列表里");
        assert_eq!(painted.color, "#aabbcc", "正常色必须原样保留");
    }

    /// get_tag_backups 的"正常路径"对照：create_tag 写 Some(color) 时,
    /// 颜色必须原样保留。如果某天被改成 unwrap_or("#000") 这类默认值,
    /// 仅靠 NULL → "" 那条测试是检测不出来的。
    #[tokio::test]
    async fn test_get_tag_backups_preserves_normal_color() {
        let db = setup_db().await;
        db.create_tag("normal", "#abc123").await.unwrap();

        let backups = db.get_tag_backups().await.unwrap();
        assert_eq!(backups.len(), 1);
        assert_eq!(backups[0].name, "normal");
        // 正常路径: 颜色原样保留,不是空串,也不是默认值
        assert_eq!(backups[0].color, "#abc123");
    }
}

// =====================================================================
// SyncRecord 模块测试
// =====================================================================

#[cfg(test)]
mod sync_record_tests {
    use super::*;

    /// 分页 + 按 id 倒序：id 是自增主键,等同于按时间倒序;
    /// limit/offset 必须严格按预期切片,这是前端分页表格的契约。
    #[tokio::test]
    async fn test_sync_records_pagination_orders_by_id_desc() {
        let db = setup_db().await;
        for i in 0..5 {
            db.create_sync_record(
                "upload",
                "skip",
                "success",
                "todos",
                Some(&format!("detail-{i}")),
                None,
            )
            .await
            .unwrap();
        }

        // 取前 2 条 —— 应当是 id=5, id=4(后插入的在前)
        let page1 = db.get_sync_records(2, 0).await.unwrap();
        assert_eq!(page1.len(), 2);
        assert!(page1[0].id > page1[1].id, "必须按 id 倒序");
        assert_eq!(page1[0].details.as_deref(), Some("detail-4"));

        // 第 2 页 —— id=3, id=2
        let page2 = db.get_sync_records(2, 2).await.unwrap();
        assert_eq!(page2.len(), 2);
        assert!(page2[0].id > page2[1].id);
        assert_eq!(page2[0].details.as_deref(), Some("detail-2"));

        // 第 3 页 —— 只剩 id=1
        let page3 = db.get_sync_records(2, 4).await.unwrap();
        assert_eq!(page3.len(), 1);
        assert_eq!(page3[0].details.as_deref(), Some("detail-0"));
    }

    /// 边界：limit=0 返回空 vec,不能因为 limit=0 而 panic 或返回全表。
    #[tokio::test]
    async fn test_sync_records_zero_limit_returns_empty() {
        let db = setup_db().await;
        db.create_sync_record("download", "overwrite", "failed", "tags", None, Some("net error"))
            .await
            .unwrap();
        let result = db.get_sync_records(0, 0).await.unwrap();
        assert!(result.is_empty(), "limit=0 必须返回空 vec");
    }

    /// count + clear 是"清空同步历史"按钮的两个原子操作:计数要对得上,
    /// clear 必须返回受影响行数(前端提示"已清除 N 条")。
    #[tokio::test]
    async fn test_sync_records_count_and_clear() {
        let db = setup_db().await;
        assert_eq!(db.count_sync_records().await.unwrap(), 0);

        db.create_sync_record("upload", "skip", "success", "todos", None, None)
            .await
            .unwrap();
        db.create_sync_record("download", "merge", "success", "tags", None, None)
            .await
            .unwrap();
        assert_eq!(db.count_sync_records().await.unwrap(), 2);

        let cleared = db.clear_sync_records().await.unwrap();
        assert_eq!(cleared, 2);
        assert_eq!(db.count_sync_records().await.unwrap(), 0);

        // 二次 clear 返回 0(没有副作用)
        assert_eq!(db.clear_sync_records().await.unwrap(), 0);
    }
}

// =====================================================================
// AgentBot 模块测试
// =====================================================================

#[cfg(test)]
mod agent_bot_tests {
    use super::*;

    /// 创建/查询/删除主链路：get_agent_bots 按 id 倒序(最新在前),
    /// get_agent_bot 按 id 精确定位,delete_agent_bot 必须能干净删除。
    #[tokio::test]
    async fn test_agent_bot_crud_main_path() {
        let db = setup_db().await;
        let id_first = db
            .create_agent_bot("custom", "first", "app1", "secret1", None, None, 0)
            .await
            .unwrap();
        let id_second = db
            .create_agent_bot("custom", "second", "app2", "secret2", None, None, 0)
            .await
            .unwrap();

        let bots = db.get_agent_bots().await.unwrap();
        assert_eq!(bots.len(), 2);
        // 按 id 倒序,最新创建的在前
        assert_eq!(bots[0].id, id_second);
        assert_eq!(bots[1].id, id_first);
        assert!(bots.iter().all(|b| b.enabled));
        assert!(bots.iter().all(|b| b.config == "{}"));

        let fetched = db.get_agent_bot(id_first).await.unwrap().unwrap();
        assert_eq!(fetched.bot_name, "first");
        assert_eq!(fetched.app_id, "app1");

        db.delete_agent_bot(id_first).await.unwrap();
        assert!(db.get_agent_bot(id_first).await.unwrap().is_none());
        // 另一个 bot 还在
        assert!(db.get_agent_bot(id_second).await.unwrap().is_some());
    }

    /// feishu 类型的 bot 在 create 时必须自动建好 p2p + group 两份 response config,
    /// 否则飞书私聊/群消息的处理会因为没有 config 行而走默认行为。
    /// 同时验证 update_agent_bot_config 写入新 JSON 字符串。
    #[tokio::test]
    async fn test_feishu_agent_bot_auto_creates_response_config() {
        let db = setup_db().await;
        let bot_id = db
            .create_agent_bot(
                "feishu",
                "feishu-bot",
                "cli_test",
                "secret_test",
                Some("ou_test_open_id".to_string()),
                Some("https://open.feishu.cn".to_string()),
                0,
            )
            .await
            .unwrap();

        let bot = db.get_agent_bot(bot_id).await.unwrap().unwrap();
        assert_eq!(bot.bot_type, "feishu");
        assert_eq!(bot.bot_open_id.as_deref(), Some("ou_test_open_id"));

        // 用 entity 直接查询验证 p2p + group 两份 response_config 都建好了
        let configs = feishu_response_config::Entity::find()
            .filter(feishu_response_config::Column::BotId.eq(bot_id))
            .all(db._conn_raw())
            .await
            .unwrap();
        assert_eq!(configs.len(), 2, "feishu bot 必须自动建 p2p + group 两份 config");

        let target_types: Vec<&str> = configs.iter().map(|c| c.target_type.as_str()).collect();
        assert!(target_types.contains(&"p2p"));
        assert!(target_types.contains(&"group"));
        assert!(configs.iter().all(|c| c.enabled));
        assert!(configs.iter().all(|c| c.debounce_secs == Some(20)));

        // 顺手验证 get_feishu_response_enabled 能直接返回 true
        assert!(db.get_feishu_response_enabled(bot_id, "p2p").await.unwrap());
        assert!(db.get_feishu_response_enabled(bot_id, "group").await.unwrap());

        // update_agent_bot_config 写入新 config 字符串,get_agent_bot 应能读到
        db.update_agent_bot_config(bot_id, r#"{"k":"v"}"#)
            .await
            .unwrap();
        let bot_after = db.get_agent_bot(bot_id).await.unwrap().unwrap();
        assert_eq!(bot_after.config, r#"{"k":"v"}"#);

        // update 一个不存在的 id 必须是 no-op:
        // 1) 不能报错(走静默 no-op 路径)
        // 2) 不能凭空插入幽灵行(get_agent_bot 仍返回 None)
        // 3) 不能影响已有行(原 bot_id 的 config 仍是上一步写入的 {"k":"v"})
        db.update_agent_bot_config(99999, "{}").await.unwrap();
        assert!(
            db.get_agent_bot(99999).await.unwrap().is_none(),
            "update 幽灵 id 不应凭空插入行"
        );
        let bot_unchanged = db.get_agent_bot(bot_id).await.unwrap().unwrap();
        assert_eq!(
            bot_unchanged.config, r#"{"k":"v"}"#,
            "update 幽灵 id 不应影响其他 bot 的 config"
        );
    }

    /// 非 feishu 类型的 bot 不应触发 response config 自动建表 —— 这条是"不能误创建"
    /// 的负向测试,避免某天误改 if 条件时悄无声息地写一堆垃圾数据。
    #[tokio::test]
    async fn test_non_feishu_agent_bot_does_not_create_response_config() {
        let db = setup_db().await;
        let bot_id = db
            .create_agent_bot("custom", "no-feishu-bot", "x", "y", None, None, 0)
            .await
            .unwrap();

        let configs = feishu_response_config::Entity::find()
            .filter(feishu_response_config::Column::BotId.eq(bot_id))
            .all(db._conn_raw())
            .await
            .unwrap();
        assert!(
            configs.is_empty(),
            "非 feishu bot 不应自动建 response_config"
        );
    }
}

// =====================================================================
// ExecutorConfig 模块测试
// =====================================================================

#[cfg(test)]
mod executor_config_tests {
    use super::*;

    /// get_enabled_executors 必须严格按 enabled=true 过滤,
    /// 这条路径是执行器派发(spawn_lifecycle)选择的真实来源 —— 错了就
    /// 派不到对应执行器,任务卡死。
    #[tokio::test]
    async fn test_get_enabled_executors_filters_by_enabled_flag() {
        let db = setup_db().await;
        // 先 seed 一份默认执行器表
        db.seed_default_executors().await.unwrap();
        let total = db.get_executors().await.unwrap();
        assert!(!total.is_empty(), "seed 后表应当非空");
        let enabled_initial = db.get_enabled_executors().await.unwrap();
        assert_eq!(enabled_initial.len(), total.len(), "默认全部 enabled");

        // 禁用一个,验证 enabled 列表少一条
        let target = &total[0];
        db.update_executor(&target.name, None, Some(false), None, None)
            .await
            .unwrap();
        let enabled_after = db.get_enabled_executors().await.unwrap();
        assert_eq!(enabled_after.len(), total.len() - 1);
        assert!(
            enabled_after.iter().all(|e| e.name != target.name),
            "被禁用的执行器不应再出现在 enabled 列表中"
        );
    }

    /// update_executor 字段级语义：只有 Some 的字段才被覆盖,None 的字段保持原值。
    /// 这是配置面板"局部保存"按钮的契约 —— 改 path 不应误清空 display_name。
    #[tokio::test]
    async fn test_update_executor_preserves_unspecified_fields() {
        let db = setup_db().await;
        db.seed_default_executors().await.unwrap();
        let before = db.get_executors().await.unwrap();
        let target = before[0].clone();

        let original_path = target.path.clone();
        let original_display_name = target.display_name.clone();

        // 只改 enabled,其他字段维持不变
        db.update_executor(&target.name, None, Some(false), None, None)
            .await
            .unwrap();
        let after = db.get_executor_by_name(&target.name).await.unwrap().unwrap();
        assert!(!after.enabled);
        assert_eq!(after.path, original_path, "未指定的 path 不应被改");
        assert_eq!(
            after.display_name, original_display_name,
            "未指定的 display_name 不应被改"
        );

        // 改 path 时不应动 enabled(已 disabled 应保持 disabled)
        db.update_executor(&target.name, Some("/usr/bin/cc-new"), None, None, None)
            .await
            .unwrap();
        let after2 = db.get_executor_by_name(&target.name).await.unwrap().unwrap();
        assert_eq!(after2.path, "/usr/bin/cc-new");
        assert!(!after2.enabled, "未指定 enabled 不应被恢复");
        assert_eq!(after2.display_name, original_display_name);

        // 不存在的 name 必须是 no-op:
        // 1) 不能报错(走静默 no-op 路径)
        // 2) 不能凭空插入幽灵行(get_executor_by_name 仍返回 None)
        // 3) 不能影响已有行(原 target.name 的 path 仍是上一步写入的 /usr/bin/cc-new)
        let before_ghost = db.get_executors().await.unwrap().len();
        db.update_executor("totally-ghost-executor", Some("/x"), None, None, None)
            .await
            .unwrap();
        assert!(
            db.get_executor_by_name("totally-ghost-executor")
                .await
                .unwrap()
                .is_none(),
            "update 幽灵 name 不应凭空插入行"
        );
        assert_eq!(
            db.get_executors().await.unwrap().len(),
            before_ghost,
            "update 幽灵 name 不应改变总行数"
        );
        let target_unchanged = db.get_executor_by_name(&target.name).await.unwrap().unwrap();
        assert_eq!(
            target_unchanged.path, "/usr/bin/cc-new",
            "update 幽灵 name 不应影响其他行的 path"
        );
    }

    /// get_executor_by_name 是 name→config 的唯一查询入口,缺失时返回 None,
    /// 不能用空 Model 误导 spawn_lifecycle。
    #[tokio::test]
    async fn test_get_executor_by_name_returns_none_for_missing() {
        let db = setup_db().await;
        db.seed_default_executors().await.unwrap();
        assert!(db
            .get_executor_by_name("definitely-not-a-real-executor")
            .await
            .unwrap()
            .is_none());
        // 真实存在的执行器应当查得到 —— 硬编码 "claudecode" 比循环
        // 断言失败信号更明确: 一旦 seed 后 EXECUTORS 任意一项被中途
        // 改 enabled=false,循环断言仍可命中剩余项,从而掩盖目标名错配。
        assert!(db
            .get_executor_by_name("claudecode")
            .await
            .unwrap()
            .is_some());
    }

    /// sync_new_executors 的语义：
    ///   - DB 缺一个新执行器 → 插入(enabled=true)
    ///   - DB 多了一个代码里没有的执行器 → 自动禁用(enabled=false)
    /// 这条是"零运维"的契约测试 —— 升级二进制后,新增的执行器自动可用,
    /// 被废弃的执行器自动关闭,不需要手动改数据库。
    /// 本测试只覆盖幂等 + enabled 集合稳定; "禁用"分支见
    /// test_sync_new_executors_disables_removed_executors。
    #[tokio::test]
    async fn test_sync_new_executors_is_idempotent_after_seed() {
        let db = setup_db().await;
        db.seed_default_executors().await.unwrap();
        let total_before = db.get_executors().await.unwrap();
        let enabled_before = db.get_enabled_executors().await.unwrap();

        // 二次跑 sync —— 必须幂等,行数与 enabled 集合不变
        db.sync_new_executors().await.unwrap();
        let total_after = db.get_executors().await.unwrap();
        let enabled_after = db.get_enabled_executors().await.unwrap();

        assert_eq!(
            total_after.len(),
            total_before.len(),
            "二次 sync 不应改变总行数"
        );
        let names_before: std::collections::HashSet<_> =
            total_before.iter().map(|e| e.name.as_str()).collect();
        let names_after: std::collections::HashSet<_> =
            total_after.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names_before, names_after, "执行器名称集合应保持稳定");
        assert_eq!(enabled_before.len(), enabled_after.len(), "enabled 集合应保持稳定");
    }

    /// sync_new_executors 的"禁用"分支：DB 里有但代码 EXECUTORS 常量里没有的
    /// executor,应当被自动标记为 enabled=false。这条覆盖 db/executor_config.rs
    /// 的 update 分支(同步禁用历史执行器),是"零运维"契约的另一半。
    #[tokio::test]
    async fn test_sync_new_executors_disables_removed_executors() {
        let db = setup_db().await;
        db.seed_default_executors().await.unwrap();

        // 注入一个"代码里没有的"历史 executor,模拟升级后被废弃的执行器
        executors::Entity::insert(executors::ActiveModel {
            name: ActiveValue::Set("legacy-deprecated".to_string()),
            path: ActiveValue::Set("/old/path".to_string()),
            enabled: ActiveValue::Set(true),
            display_name: ActiveValue::Set("legacy".to_string()),
            session_dir: ActiveValue::Set(String::new()),
            ..Default::default()
        })
        .exec(db._conn_raw())
        .await
        .unwrap();

        // 同步:代码里没有的 executor 必须被自动禁用
        db.sync_new_executors().await.unwrap();
        let legacy = db
            .get_executor_by_name("legacy-deprecated")
            .await
            .unwrap()
            .expect("注入的 legacy executor 应当在表中");
        assert!(
            !legacy.enabled,
            "sync_new_executors 必须自动禁用代码里已删除的 executor"
        );
    }

    /// migrate_from_config 只在 executors 表为空时生效,
    /// 且使用 cfg_executors.paths 里配置的 path(若存在),否则回退到 default_path。
    #[tokio::test]
    async fn test_migrate_from_config_only_runs_when_empty() {
        let db = setup_db().await;

        // 表为空时,自定义 path 应当被写入
        let mut paths = HashMap::new();
        paths.insert("claudecode".to_string(), "/custom/path/claude".to_string());
        let cfg = ExecutorPaths { paths };
        db.migrate_from_config(&cfg).await.unwrap();

        let after = db.get_executors().await.unwrap();
        let claude = after
            .iter()
            .find(|e| e.name == "claudecode")
            .expect("claudecode 应当被 seed");
        assert_eq!(claude.path, "/custom/path/claude");
        assert!(claude.enabled);

        // 再 migrate 一次 —— 表已非空,不会再写入;之前自定义的 path 保留
        let mut paths2 = HashMap::new();
        paths2.insert("claudecode".to_string(), "/another/path".to_string());
        let cfg2 = ExecutorPaths { paths: paths2 };
        db.migrate_from_config(&cfg2).await.unwrap();
        let after2 = db.get_executors().await.unwrap();
        let claude2 = after2
            .iter()
            .find(|e| e.name == "claudecode")
            .unwrap();
        assert_eq!(
            claude2.path, "/custom/path/claude",
            "表非空时 migrate_from_config 应当是 no-op"
        );
    }
}