# ntd-connect backend 适配 dry-run

> 作者：Claude（MiniMax-M3）
> 日期：2026-06-28
> 状态：调研完成，待用户拍板后执行步骤 4 起

把现有 `backend/src/services/feishu_listener.rs`（1853 行）拆开：协议层（HTTP + WS）→ `ntd-connect::platform::feishu`；派发层 → `ntd-connect::Dispatcher`；业务路由层 → backend 新 `MessageRouter`。

## 核心拆分

| 职责 | 迁到哪 | 当前实现 |
|------|--------|----------|
| WS 收消息 + HTTP 调飞书（reaction / send_raw / get_tenant_token）| `ntd-connect::platform::feishu` | M3 stub（HTTP 已实现，WS 待 v2 接 backend ws_client）|
| per-session 锁 + busy queue + watermark | `ntd-connect::Dispatcher` | M2 已实现 |
| builtin 命令 (/sethome /bind 等) + 过滤器 + binding/slash/default | backend `MessageRouter`（新 service）| 保留 + 抽出 |

**保留不动**：`MessageDebounce`、`FeishuHistoryFetcher`（正交入站路径，与 Dispatcher 共用 debounce）。

## 多 bot 拓扑

**单 Dispatcher + ChannelRegistry**（`Arc<DashMap<bot_id, Arc<FeishuPlatform>>>`），**不**每 bot 一个 Dispatcher（浪费 LRU session）。

## 迁移顺序（10 步）

| 步 | 工作 | 风险 | 状态 |
|----|------|------|------|
| 1-3 | ntd-connect M1/M2/M3 + workspace | — | ✅ 已完成 |
| **4** | `backend/Cargo.toml` 加 `ntd-connect` dep | 低 | 待做 |
| **5** | `build_app_state` 构造 `SharedHttpClient` + `ChannelRegistry` + dummy MessageHandler；不动 dispatcher，先验证 bot 创建/start | 低 | 待做 |
| **6** | 把 feishu_listener HTTP helpers 抽到 backend `FeishuApiClient` thin wrapper，转发到 ntd-connect；改 feishu_push.rs / feishu_history_fetcher.rs | 中 | 待做 |
| **7** | `FeishuPlatform::start` 接 backend `FeishuChannelService.listen(tx)`，`ChannelMessage` → `IncomingMessage` 调 handler.on_message；dummy handler | 中 | 待做 |
| **8** | 抽 `MessageRouter`（backend）；7 阶段逻辑搬到 router；路由结果仍走老路径（debounce / builtin）| **高**（最大逻辑改动）| 待做 |
| **9** | 真 `Dispatcher`；所有 channel 指向它；worker 内部调 `MessageRouter`，**不接 agent** | 中 | 待做 |
| **10** | M4（Claude Code executor）完成后，worker 接真 agent | 高 | 待 M4 |
| **11** | 删 `feishu_listener.rs`；清理 AppState.feishu_listener | 低 | 待做 |
| **12** | v2：暴露 `FeishuPlatform::get_token()` 给 backend；删 backend `TokenManager` | 低 | v2 |

## 关键调用方改造

| 文件 | 行 | 改造 |
|------|---|------|
| `backend/src/handlers/mod.rs:build_app_state` | 1025-1075 | 构造 SharedHttpClient + ChannelRegistry；AppState 加 `dispatcher` + `channel_registry`，删 `feishu_listener` |
| `backend/src/handlers/agent_bot.rs` | 164, 310-313, 492, 911-920 | `start_bot / has_bot` 改 `ChannelRegistry::register / contains_key` |
| `backend/src/services/feishu_push.rs` | 62, 75, 112 | `send_raw(bot_id, receive_id, receive_id_type, content)` → `registry.get(&bot_id).send(&ctx, ReplyTarget::Feishu{...}, OutgoingContent::Text(content))` |

## 风险与缓解

### 双跑避免双重处理
- v1 单步切流（用户早先选择）；上线当天消息可能跳一下
- 缓解：步骤 5-8 期间 `feishu_listener` 仍跑，dispatcher 是只读验证；步骤 9 才开始接入；步骤 11 一次性删老

### Token 缓存重复
- v1：backend `TokenManager` + ntd-connect 内部 token 缓存并存，各 2h 刷一次（多浪费一次 HTTP /2h /bot）
- v2（步骤 12）：统一

### Session state 重启丢失
- Dispatcher 重启 → SessionState 内存清空
- 缓解：backend DB `binding.session_id` + `execution_records.session_id` 作为恢复源；下次消息进 dispatcher 重建 session（watermark=-1，全放行）

### Token / reaction 失败 fallback
- M3 v1 失败仅 `tracing::warn!`，不 panic
- Dispatcher 启动失败：v1 不做自动重连（步骤 5 验证点之一）
- Graceful shutdown：`Dispatcher::join()` 已实现，main.rs Ctrl-C 时调用

## 不动的部分（保命）

- `backend/src/services/message_debounce.rs`：业务路由层调它，跟 Dispatcher 解耦
- `backend/src/services/feishu_history_fetcher.rs`：独立后台 task，正交于 Dispatcher
- `backend/src/services/loop_runner.rs`：通过 debounce 间接触发，不感知 dispatcher
- DB schema（`feishu_messages` / `feishu_history_chats` / `agent_bots` 等）：不变
- HTTP API 协议：不变
- 前端 `MessagesPanel` / `BotDetailPage`：不变
- `backend/src/feishu/` 整个 module（WS client / token_manager / message.rs）：保留，FeishuPlatform v2 接入 WS 时复用

## 关键文件清单（绝对路径）

### 待改 / 待删（backend）
- `backend/src/services/feishu_listener.rs` — **主改造目标**（最终删除）
- `backend/src/services/feishu_push.rs` — send_raw 调用点改造
- `backend/src/handlers/agent_bot.rs` — start_bot/has_bot 改造
- `backend/src/handlers/mod.rs` — build_app_state + AppState
- `backend/src/services/mod.rs` — 删 feishu_listener pub mod
- `backend/Cargo.toml` — 加 ntd-connect dep

### 已就绪（ntd-connect）
- `ntd-connect/src/channel.rs` / `dispatcher.rs` / `session.rs` / `types.rs` / `agent.rs` / `typing.rs` / `dedup.rs` / `http.rs` / `error.rs` / `platform/feishu.rs`
- M3 v1 是 placeholder；步骤 7 接入 backend ws_client

## 当前假设（用户答完没显示，按此前偏好推进）

- router 拆分：抽 `MessageRouter`（最小改动，保留 7 阶段逻辑）
- v1 切流：单步（与早先选择一致）
- token 缓存：v1 各缓存一份，v2 再统一
- M4 executor：暂不接真 agent，先把 MessageRouter 跑通（步骤 8-9 即可上线）

如果假设不对，请告诉我具体偏好，否则我按这个推进步骤 4-9。
