# 100 — FeishuListener 拆分设计（096-W3-PR4）

## 背景与目标

`services/feishu_listener.rs`（2600 行、82 函数）七职责合一：WS 连接管理 / 凭证 token /
消息编排 / 斜杠命令 / 卡片回调 / 卡片组装 / HTTP 发送（扫描报告 C2）。
本设计按 096 方案 W3-PR4 落拆分类名与字段归属，作为 PR 评审的边界依据。

**关键事实（降低风险的基础）**：C/D/E 三族函数全部为**无 self 关联函数**
（接 `FeishuCommandContext`/`ListenerMessageContext` 参数对象或显式传 credentials），
本次拆分 = 整段剪切 + 调用点 `Self::` 改路径，函数体零改动。
仅 A/B 族（连接管理 + 消息编排入口）持有 `&self` 状态（channels/debounce），留守 listener。

## 拆分类名与文件落点（平铺，与 feishu_push/feishu_card 风格一致）

| 新模块 | 类名 | 职责域 |
|---|---|---|
| `services/feishu_api_client.rs` | `FeishuApiClient`（unit struct + 关联函数） | HTTP 发送 + 凭证/token + reaction |
| `services/feishu_slash_commands.rs` | `SlashCommandHandler`（unit struct + 关联函数） | /new /list /stop /help /feishupush 五命令 + help 卡片辅助 |
| `services/feishu_card_actions.rs` | `CardActionHandler`（unit struct + 关联函数） | 卡片回调路由 + act: 动作族 + 卡片 patch/历史分页 |
| `services/feishu_listener.rs`（留守） | `FeishuListener`（&self 方法族） | WS 连接管理 + 消息编排入口流水线 |

共享类型（`ListenerMessageContext`/`FeishuCommandContext`/`CardAction`/`ActionOutcome`/
`SlashCommandMatch`/`HistoryItem`/`HelpCardState` 等）留 `feishu_listener.rs`（`pub(crate)`），
各新模块 `use` 引入——类型定义不搬迁，缩小 diff 面。

## 字段归属表

| 字段 | 归属 | 理由 |
|---|---|---|
| `channels`（WS 连接） | FeishuListener | 连接管理本职；`send()`（&self 走 channel）留守 |
| `debounce` | FeishuListener | 消息编排（debounce_push_*）本职 |
| `token_manager` / `bot_credentials` | 显式传参（不进任何 struct 字段） | E 族函数现状即显式接参，保持 |
| `ctx: ServiceContext` | 经 context 参数对象流转 | 现状如此，不变 |

## 函数归域清单（82 个）

**留守 FeishuListener（A 连接管理 + B 消息编排，21 个）**：
new / has_bot / start_bot / handle_message / prepare_message /
persist_inbound_message / capture_owner_if_p2p / add_processing_reaction /
try_route_builtin_command / should_skip_for_message_filters / is_group_sender_allowed /
cleanup_reaction / route_slash_or_default_response / dispatch_slash_command / find_slash_rule /
push_slash_command_message / push_slash_command_loop_message / dispatch_default_response /
debounce_push_default / debounce_push_executor_default / debounce_push_loop_default /
log_echo_reply / is_message_allowed / parse_slash_command（静态解析留入口近旁）

**FeishuApiClient（E 发送+凭证，13 个）**：
patch_card / send_text / send_card / reply_card / send_raw / send_card_raw /
base_url / build_sdk_config / get_tenant_token / resolve_bot_open_id /
add_reaction / delete_reaction
（`send()` 走 channels 的 &self 方法留守 listener，不在此列）

**SlashCommandHandler（C 斜杠命令，11 个）**：
handle_feishupush / handle_list / handle_new / handle_stop / handle_help /
assemble_help_card_state / build_workspace_summary / recent_records_and_running /
brief_to_item / loop_to_item / record_to_recent_item

**CardActionHandler（D 卡片回调，24 个）**：
handle_card_callback / handle_nav_action / execute_card_action / run_card_action /
action_target_group / act_push / act_new / act_stop / act_bind / act_run_todo /
act_run_loop / act_set_executor / registered_executor_names / patch_after_action /
patch_rendered_card / patch_history_page / query_history / record_to_history_item /
parse_card_command / parse_card_action / resolve_receive_target /
workspace_default_executor / ensure_default_response_executor / format_record_time
（纯工具，C/D 共用，随 D 族主场）

## 调用关系与可见性

- 三族函数统一 `pub(crate)`（现状 impl 内私有，搬出后需 crate 内可达）。
- 调用点改写：listener 内 `Self::send_text(...)` → `FeishuApiClient::send_text(...)`；
  `Self::handle_new(...)` → `SlashCommandHandler::handle_new(...)` 等。
- 测试随迁：`parse_card_command`/`parse_card_action`/`resolve_receive_target` 等
  纯函数测试随 D 族；`parse_slash_command`/`is_message_allowed` 测试留 listener。

## 验证方案

1. `cargo clippy --all-targets -- -D warnings` 零告警；全量 `cargo test` 通过。
2. 搬迁等价性：函数体逐字 diff 核验（源区段 vs 新文件）。
3. **真实通路冒烟**（低峰窗口执行）：dev 实例验证飞书通路——
   若环境无可用 bot 凭证，则以「连接建立 + 消息入口日志 + 既有集成测试」为下限，
   并在 PR 中如实声明冒烟覆盖度。
