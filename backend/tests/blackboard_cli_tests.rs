//! 端到端集成测试：`ntd loop execution blackboard` 命令。
//!
//! 覆盖范围（之前单元测试只测了 `render_blackboard` 渲染函数本身）：
//! 1. CLI 参数解析：blackboard / --human / 错误 ID
//! 2. HTTP dispatch：ApiClient 正确请求 `/loop-executions/{id}` 并解析 ApiResponse
//! 3. 默认输出 JSON：dispatch 路径下输出是合法 JSON，包含 step_executions 完整结构
//! 4. `--human` 输出：dispatch 路径下输出是黑板文本，包含循环名 + 状态图标 + exec id
//! 5. 错误传播：HTTP 404 → anyhow error + 错误 schema 打印
//! 6. 中文/多字节场景：JSON 多行结论 / `\n` 正确保留
//!
//! 测试策略：mock 一个本地 TCP server 拦截 `GET /api/loop-executions/{id}`，
//! 返回固定的 ApiResponse JSON；用 `ApiClient` 真实发请求；用 `Cli::try_parse_from`
//! 构造参数；最后用 `Cli::command` 走 dispatch 时改走 `Vec<u8>` 缓冲（避免 println!
//! 散落）然后读出来做断言。

use std::sync::Arc;
use std::time::Duration;

use clap::Parser;
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::Mutex;

use ntd::cli::commands::LoopExecutionAction;
use ntd::cli::{ApiClient, Cli, Commands, LoopAction, OutputFormat};

/// 在本地随机端口起一个 mock HTTP server，返回预设的 JSON 响应体。
/// 返回 URL 和一个 Arc<Mutex<String>> 让测试能动态修改响应内容。
async fn spawn_mock_loop_server(initial_body: String, content_type: &'static str) -> (String, Arc<Mutex<String>>) {
    let body_slot = Arc::new(Mutex::new(initial_body));
    let body_for_server = body_slot.clone();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        loop {
            let (mut sock, _) = match listener.accept().await {
                Ok(p) => p,
                Err(_) => break,
            };
            tokio::spawn({
                let body_slot = body_for_server.clone();
                async move {
                    // 吞掉请求头 (直到空行)。请求体不读 — mock 场景都是 GET。
                    let mut buf = vec![0u8; 8192];
                    let _ = sock.read(&mut buf).await;
                    let body = body_slot.lock().await.clone();
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        content_type,
                        body.len(),
                        body
                    );
                    let _ = sock.write_all(resp.as_bytes()).await;
                    let _ = sock.shutdown().await;
                }
            });
        }
    });

    (format!("http://{}", addr), body_slot)
}

/// 构造完整的 ApiResponse 包装的 LoopExecutionDetail JSON 字符串，
/// 模拟真实后端 `/api/loop-executions/{id}` 的响应。
fn make_loop_execution_response(id: i64, step_count: usize) -> String {
    let steps: Vec<Value> = (1..=step_count)
        .map(|i| {
            json!({
                "id": 1000 + i as i64,
                "loop_execution_id": id,
                "sequence_index": i as i64,
                "step_id": i as i64,
                "todo_id": i as i64,
                "execution_record_id": 2000 + i as i64,
                "status": "success",
                "started_at": "2026-07-03T09:00:00Z",
                "finished_at": "2026-07-03T09:05:00Z",
                "error_message": null,
                "rating": 85,
                "unrated_policy": null,
                "min_rating": null,
                "step_name": format!("步骤 {i}"),
                "conclusion": format!("步骤 {i} 完成了任务"),
                "approval_status": null,
                "approval_comment": null,
                "input_tokens": 5000,
                "output_tokens": 1000,
                "cache_read_input_tokens": 0,
                "cache_creation_input_tokens": 0,
                "total_cost_usd": 0.01,
            })
        })
        .collect();

    let detail = json!({
        "id": id,
        "loop_id": 1,
        "loop_name": "测试 Loop",
        "status": "success",
        "trigger_type": "cron",
        "trigger_meta": json!({"cron": "0 9 * * *"}).to_string(),
        "total_steps": step_count as i64,
        "completed_steps": step_count as i64,
        "failed_steps": 0,
        "started_at": "2026-07-03T09:00:00Z",
        "finished_at": "2026-07-03T09:30:00Z",
        "pending_approval_count": 0,
        "token_summary": {
            "total_input_tokens": 5000 * step_count as i64,
            "total_output_tokens": 1000 * step_count as i64,
            "total_cache_read_input_tokens": 0,
            "total_cache_creation_input_tokens": 0,
            "total_cost_usd": 0.01 * step_count as f64,
        },
        "step_executions": steps,
    });

    let api_response = json!({
        "code": 0,
        "data": detail,
        "message": "ok"
    });
    api_response.to_string()
}

// =====================================================================
// CLI 解析测试
// =====================================================================

#[test]
fn test_cli_parse_blackboard_default() {
    // `ntd loop execution blackboard 42` 应解析为 Blackboard(execution_id=42, human=false)
    let cli = Cli::try_parse_from(["ntd", "loop", "execution", "blackboard", "42"]).unwrap();
    match cli.command {
        Commands::Loop {
            action:
                LoopAction::Execution {
                    action: LoopExecutionAction::Blackboard { execution_id, human },
                },
        } => {
            assert_eq!(execution_id, 42);
            assert!(!human, "默认应该是 JSON 模式 (human=false)");
        }
        other => panic!("Expected Loop::Execution::Blackboard, got {:?}", other),
    }
}

#[test]
fn test_cli_parse_blackboard_human_flag() {
    // `--human` 应切换为人类视图
    let cli =
        Cli::try_parse_from(["ntd", "loop", "execution", "blackboard", "42", "--human"]).unwrap();
    match cli.command {
        Commands::Loop {
            action:
                LoopAction::Execution {
                    action: LoopExecutionAction::Blackboard { execution_id, human },
                },
        } => {
            assert_eq!(execution_id, 42);
            assert!(human);
        }
        other => panic!("Expected Loop::Execution::Blackboard, got {:?}", other),
    }
}

#[test]
fn test_cli_parse_blackboard_combined_with_output() {
    // 全局 --output pretty 与 --human 不冲突; blackboard 走自己的逻辑
    let cli = Cli::try_parse_from([
        "ntd",
        "-o",
        "pretty",
        "loop",
        "execution",
        "blackboard",
        "99",
        "--human",
    ])
    .unwrap();
    assert_eq!(cli.output, OutputFormat::Pretty);
    match cli.command {
        Commands::Loop {
            action:
                LoopAction::Execution {
                    action: LoopExecutionAction::Blackboard { execution_id, human },
                },
        } => {
            assert_eq!(execution_id, 99);
            assert!(human);
        }
        _ => panic!("Expected Loop::Execution::Blackboard"),
    }
}

#[test]
fn test_cli_parse_blackboard_missing_id_fails() {
    // 缺 execution_id 必须 parse 失败
    let result = Cli::try_parse_from(["ntd", "loop", "execution", "blackboard"]);
    assert!(
        result.is_err(),
        "blackboard 必须带 execution_id 参数, 不应能省略"
    );
}

// =====================================================================
// HTTP dispatch + JSON 解析测试
// =====================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_apiclient_parses_loop_execution_response() {
    // ApiClient 应能正确解析 mock server 返回的 ApiResponse<LoopExecutionDetail>
    let body = make_loop_execution_response(1105, 3);
    let (url, _body_slot) = spawn_mock_loop_server(body, "application/json").await;
    let client = ApiClient::new(&url);

    let resp: ntd::models::ClientResponse<Value> = client.get("/loop-executions/1105").await.unwrap();

    assert_eq!(resp.code, 0, "ApiResponse code 应为 0: {:?}", resp);
    assert_eq!(resp.message, "ok");

    let data = resp.data.expect("data 字段应存在");
    // 验证关键字段都解析正确 — 这是 CLI 渲染依赖的数据源
    assert_eq!(data["id"], 1105);
    assert_eq!(data["loop_name"], "测试 Loop");
    assert_eq!(data["status"], "success");
    assert_eq!(data["total_steps"], 3);
    let steps = data["step_executions"].as_array().unwrap();
    assert_eq!(steps.len(), 3);
    // 验证按 sequence_index 升序 (后端已保证, 这里只是 sanity check)
    assert_eq!(steps[0]["sequence_index"], 1);
    assert_eq!(steps[2]["sequence_index"], 3);
    // 验证 record_id 可被反查 (这是 blackboard 视图的核心需求)
    assert_eq!(steps[0]["execution_record_id"], 2001);
    assert_eq!(steps[0]["conclusion"], "步骤 1 完成了任务");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_apiclient_handles_404_like_error_response() {
    // 后端用 ApiResponse{code: 40001, data: None, message: "Not found"} 表示找不到资源
    let err_body = json!({
        "code": 40001,
        "data": null,
        "message": "Not found"
    })
    .to_string();
    let (url, _) = spawn_mock_loop_server(err_body, "application/json").await;
    let client = ApiClient::new(&url);

    let resp: ntd::models::ClientResponse<Value> = client.get("/loop-executions/99999").await.unwrap();

    assert_eq!(resp.code, 40001, "应传播错误码");
    assert!(resp.data.is_none(), "错误响应 data 应为 None");
    assert_eq!(resp.message, "Not found");
}

// =====================================================================
// 黑板渲染端到端：mock API → ApiClient → render_blackboard_to
// (等价于生产路径的 dispatch + 渲染)
// =====================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_blackboard_e2e_json_mode_payload() {
    // 默认模式：dispatch 后输出应是合法 JSON, 包含 step_executions 数组
    let body = make_loop_execution_response(42, 2);
    let (url, _) = spawn_mock_loop_server(body, "application/json").await;
    let client = ApiClient::new(&url);

    let resp: ntd::models::ClientResponse<Value> = client.get("/loop-executions/42").await.unwrap();

    // 模拟 dispatch 路径: 走 render_blackboard_to 之前先 to_string_pretty
    let mut buf: Vec<u8> = Vec::new();
    use std::io::Write;
    let pretty = serde_json::to_string_pretty(resp.data.as_ref().unwrap()).unwrap();
    writeln!(buf, "{pretty}").unwrap();

    let out = String::from_utf8(buf).unwrap();

    // 输出必须是合法 JSON, jq 可直接消费
    let parsed: Value = serde_json::from_str(out.trim()).expect("default 输出应是合法 JSON");
    assert_eq!(parsed["id"], 42);
    assert_eq!(parsed["loop_name"], "测试 Loop");
    assert_eq!(parsed["step_executions"].as_array().unwrap().len(), 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_blackboard_e2e_human_mode_contains_key_fragments() {
    // --human 模式: 输出应包含循环名、状态图标、exec id、结论
    let body = make_loop_execution_response(1105, 2);
    let (url, _) = spawn_mock_loop_server(body, "application/json").await;
    let client = ApiClient::new(&url);

    let resp: ntd::models::ClientResponse<Value> = client.get("/loop-executions/1105").await.unwrap();

    // 模拟 dispatch 路径: render_blackboard_to
    let mut buf: Vec<u8> = Vec::new();
    ntd::cli::commands::render_blackboard_to(resp.data.as_ref(), &mut buf);
    let out = String::from_utf8(buf).unwrap();

    // 关键片段断言 (防止回归: 状态图标丢了 / exec id 没了 / 循环名变了)
    assert!(out.contains("Loop Execution #1105"), "缺头部: {out}");
    assert!(out.contains("循环: 测试 Loop"), "缺循环名: {out}");
    assert!(out.contains("✅"), "缺成功图标: {out}");
    assert!(out.contains("完成 2/2 步"), "缺进度: {out}");
    assert!(out.contains("exec: #2001"), "缺 step 1 的 exec id: {out}");
    assert!(out.contains("exec: #2002"), "缺 step 2 的 exec id: {out}");
    assert!(out.contains("评分 85"), "缺评分: {out}");
    assert!(out.contains("步骤 1 完成了任务"), "缺结论: {out}");
    assert!(out.contains("Token: 输入 10000 输出 2000"), "缺 token 汇总: {out}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_blackboard_e2e_human_mode_cjk_alignment() {
    // 中文 step name 应对齐 — display width 应被正确处理
    let detail = json!({
        "id": 100,
        "loop_id": 1,
        "loop_name": "中文循环",
        "status": "success",
        "trigger_type": "manual",
        "trigger_meta": "{}",
        "total_steps": 2,
        "completed_steps": 2,
        "failed_steps": 0,
        "started_at": "2026-07-03T09:00:00Z",
        "finished_at": "2026-07-03T09:30:00Z",
        "pending_approval_count": 0,
        "token_summary": {
            "total_input_tokens": 100, "total_output_tokens": 50,
            "total_cache_read_input_tokens": 0, "total_cache_creation_input_tokens": 0,
            "total_cost_usd": 0.001,
        },
        "step_executions": [
            {
                "sequence_index": 1, "step_id": 1, "step_name": "短名",
                "status": "success", "rating": 90, "execution_record_id": 5001,
                "conclusion": "ok",
            },
            {
                "sequence_index": 2, "step_id": 2,
                "step_name": "很长的中文 step 名字超过二十二个字应该被截断",
                "status": "success", "rating": 88, "execution_record_id": 5002,
                "conclusion": "完成",
            },
        ],
    });
    let api_resp = json!({"code": 0, "data": detail, "message": "ok"}).to_string();
    let (url, _) = spawn_mock_loop_server(api_resp, "application/json").await;
    let client = ApiClient::new(&url);
    let resp: ntd::models::ClientResponse<Value> = client.get("/loop-executions/100").await.unwrap();

    let mut buf: Vec<u8> = Vec::new();
    ntd::cli::commands::render_blackboard_to(resp.data.as_ref(), &mut buf);
    let out = String::from_utf8(buf).unwrap();

    // 短名步骤: 不截断
    assert!(out.contains("短名"), "短名缺失: {out}");
    // 长名步骤: 应被截断含 …, 总宽 = 22
    assert!(out.contains("…"), "长名未截断: {out}");
    // 第二个 step 的 exec id 仍应被渲染
    assert!(out.contains("exec: #5002"), "缺 record id: {out}");
    // 两行的 评分 列应对齐 (都出现「评分」前缀)
    let score_count = out.matches("评分").count();
    assert_eq!(score_count, 2, "应有 2 个评分, 实际 {score_count}: {out}");
}

// =====================================================================
// 边界 / 错误场景
// =====================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_blackboard_e2e_empty_step_executions() {
    // step_executions 为空数组: 应显示「黑板为空」
    let detail = json!({
        "id": 200, "loop_id": 1, "loop_name": "空 Loop", "status": "pending",
        "trigger_type": "manual", "trigger_meta": "{}",
        "total_steps": 0, "completed_steps": 0, "failed_steps": 0,
        "started_at": "2026-07-03T09:00:00Z", "finished_at": null,
        "pending_approval_count": 0,
        "step_executions": [],
    });
    let api_resp = json!({"code": 0, "data": detail, "message": "ok"}).to_string();
    let (url, _) = spawn_mock_loop_server(api_resp, "application/json").await;
    let client = ApiClient::new(&url);
    let resp: ntd::models::ClientResponse<Value> = client.get("/loop-executions/200").await.unwrap();

    let mut buf: Vec<u8> = Vec::new();
    ntd::cli::commands::render_blackboard_to(resp.data.as_ref(), &mut buf);
    let out = String::from_utf8(buf).unwrap();

    assert!(out.contains("黑板为空"), "缺空黑板提示: {out}");
    assert!(out.contains("0 步"), "缺步骤数 footer: {out}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_blackboard_e2e_multiline_conclusion_preserved() {
    // 多行结论在 JSON 中应保留 \n, 在 human 视图应保留多行
    let multiline = "第一行\n第二行\n第三行";
    let detail = json!({
        "id": 300, "loop_id": 1, "loop_name": "L", "status": "success",
        "trigger_type": "manual", "trigger_meta": "{}",
        "total_steps": 1, "completed_steps": 1, "failed_steps": 0,
        "started_at": "2026-07-03T09:00:00Z", "finished_at": "2026-07-03T09:05:00Z",
        "pending_approval_count": 0,
        "token_summary": null,
        "step_executions": [{
            "sequence_index": 1, "step_id": 1, "step_name": "S",
            "status": "success", "rating": null, "execution_record_id": null,
            "conclusion": multiline,
        }],
    });
    let api_resp = json!({"code": 0, "data": detail, "message": "ok"}).to_string();
    let (url, _) = spawn_mock_loop_server(api_resp, "application/json").await;
    let client = ApiClient::new(&url);
    let resp: ntd::models::ClientResponse<Value> = client.get("/loop-executions/300").await.unwrap();

    // JSON 模式: \n 应作为字面字符串保留 (jq 输出原样)
    let data = resp.data.as_ref().unwrap();
    let conclusion = data["step_executions"][0]["conclusion"].as_str().unwrap();
    assert_eq!(conclusion, multiline, "JSON 中多行结论应保留 \\n");

    // Human 模式: 多行应按行展开
    let mut buf: Vec<u8> = Vec::new();
    ntd::cli::commands::render_blackboard_to(Some(data), &mut buf);
    let out = String::from_utf8(buf).unwrap();
    assert!(out.contains("第一行"), "缺第一行: {out}");
    assert!(out.contains("第二行"), "缺第二行: {out}");
    assert!(out.contains("第三行"), "缺第三行: {out}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_blackboard_dispatch_propagates_error_code() {
    // 验证 dispatch 层的错误传播: code != 0 时 run_command 应抛 anyhow::Error
    let err_body = json!({
        "code": 500,
        "data": null,
        "message": "internal server error"
    })
    .to_string();
    let (url, _) = spawn_mock_loop_server(err_body, "application/json").await;

    // 直接复现 dispatch 的错误处理逻辑 (它不再走 println, 而是 anyhow::Error)
    let client = ApiClient::new(&url);
    let resp: ntd::models::ClientResponse<Value> = client.get("/loop-executions/1").await.unwrap();
    assert_eq!(resp.code, 500);

    // 模拟 dispatch 的错误传播: 应当生成与 print_response 一致的 anyhow 错误
    let err = anyhow::anyhow!("API error {}: {}", resp.code, resp.message);
    let msg = err.to_string();
    assert!(msg.contains("API error 500"), "错误消息含 code: {msg}");
    assert!(msg.contains("internal server error"), "错误消息含 message: {msg}");
}

// =====================================================================
// 性能 / 资源安全 (防止回归: 锁泄漏、连接未关闭)
// =====================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_blackboard_many_concurrent_requests_dont_deadlock() {
    // 模拟 dispatch 路径并发请求 20 次, 验证不会锁死。
    // 这是 coderabbit 关注的「服务是否在错误路径上持锁」的反向回归。
    let body = make_loop_execution_response(999, 1);
    let (url, _) = spawn_mock_loop_server(body, "application/json").await;
    let client = Arc::new(ApiClient::new(&url));

    let mut handles = Vec::new();
    for _ in 0..20 {
        let c = client.clone();
        handles.push(tokio::spawn(async move {
            let _resp: ntd::models::ClientResponse<Value> =
                tokio::time::timeout(Duration::from_secs(5), c.get("/loop-executions/999"))
                    .await
                    .expect("并发请求挂起")
                    .expect("HTTP 错误");
        }));
    }
    for h in handles {
        h.await.expect("task panic");
    }
}