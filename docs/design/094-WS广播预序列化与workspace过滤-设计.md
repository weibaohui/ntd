# 094-WS广播预序列化与workspace过滤-设计

| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| AI (zhanlu) | 2026-08-10 | 初始版本 |

> 093 优化扫描专项·性能类第 1 项（首轮扫描 P1）。
> 实证：每条执行事件 N 个 WS 客户端（标签页）各自序列化一次；所有 workspace 的事件推给所有客户端。

## 1. 现状与证据

### 1.1 逐客户端重复序列化

`handlers/mod.rs::events_handler` 主循环（~L275）：

```rust
Ok(event) => {
    let json = serde_json::to_string(&event).unwrap_or_default();  // 每客户端各序列化一次
    ws.send(Message::Text(json.into()))                            // 每客户端各分配一份 String
}
```

N 个标签页 = 同一事件 N 次序列化 + N 次堆分配。执行器日志洪峰期（Output 事件每秒数十条）
放大为 CPU 尖峰。

### 1.2 无 workspace 过滤

- 主循环：`broadcast::Receiver<ExecEvent>` 收到什么发什么，不看 `workspace_id`；
- Sync 握手：`task_manager.get_all_task_infos()` 全量返回，不按 workspace 过滤
  （`TaskInfo` 无 workspace 字段，需经 execution_record 反查）；
- 前端 `useExecutionEvents.ts`：`new WebSocket("/api/events")` 无参数，
  全局 dispatch 不按 workspace 区分——**跨 workspace 的任务标题/日志流对任意面板可见
  （轻度租户信息泄露），本设计顺带消除**。

### 1.3 附带浪费

`DirectCardMessage`/`DirectStreamMessage` 是飞书专用事件（bot_id/receive_id 定向），
WS 客户端收到后前端 switch 无对应分支，纯带宽浪费。

## 2. 方案选型

### 入选：forwarder + WS 专用 channel（最小侵入）

```
ExecEvent channel (state.tx)      ← 现有链路零改动（12 发送点 + 飞书订阅侧）
   │
   ▼ forwarder 任务（main.rs 启动）
   跳过飞书专用事件 → serde_json::to_string 一次 → ws_tx.send(WsEnvelope)
   │
WsEnvelope { workspace_id: Option<i64>, json: Arc<str> }   ← 全局唯一序列化产物
   │
   ▼ events_handler（/api/events?workspace_id=N）
   envelope.workspace_id 与连接声明匹配才转发；Arc<str> 全客户端共享
```

### 落选：改原 channel 为 envelope

`state.tx: broadcast::Sender<(ExecEvent, Arc<str>)>` 一步到位，但 12 个发送文件 +
`feishu_push` 订阅侧全要改，侵入面 3 倍于入选方案，且飞书路径不需要 JSON（白序列化）。
落选理由：YAGNI + 最小侵入。

### 关键取舍

- **预序列化时机**：forwarder 无条件序列化（即使零 WS 客户端）。
  序列化微秒级，换来 channel 消息语义单一，值得；
- **零拷贝级别**：`Arc<str>` 共享，WS 发送时 `Message::text(&*json)` 一次 memcpy。
  彻底零拷贝（Bytes/Utf8Bytes 直进 channel）留作后续——memcpy 相对序列化成本可忽略；
- **兼容性**：无 `workspace_id` 参数的连接全推（旧前端/第三方客户端行为不变）；
- **全局事件**：`workspace_id()` 返回 None 的变体（ReviewStatusChanged 等）全推——
  这些事件不含 workspace 敏感信息（经用户确认的决策点 3a）；
- **Sync 异常任务**：DB 降级查不到 record 的任务保守保留（决策点 2a）。

## 3. 设计明细

### C1：`ExecEvent` 查询方法（events.rs）

```rust
impl ExecEvent {
    /// 事件的 workspace 归属：None = 全局事件（Sync/ReviewStatusChanged/飞书直发）
    pub fn workspace_id(&self) -> Option<i64>;

    /// 飞书专用事件（DirectCardMessage/DirectStreamMessage）：WS 客户端不消费
    pub fn is_feishu_direct(&self) -> bool;
}
```

13 变体全枚举（漏一个编译器会报 non-exhaustive，天然守卫）。

### C2：WsEnvelope + forwarder（handlers/mod.rs 或独立模块）

```rust
/// WS 广播信封：json 为预序列化产物（全局一份），workspace_id 为过滤键
#[derive(Debug, Clone)]
pub struct WsEnvelope {
    pub workspace_id: Option<i64>,
    pub json: Arc<str>,
}

/// 启动转发任务：订阅 ExecEvent channel → 过滤飞书专用 → 预序列化 → 发 WS channel
pub fn spawn_ws_forwarder(tx: broadcast::Sender<ExecEvent>, ws_tx: broadcast::Sender<WsEnvelope>);
```

`AppState` 增加 `ws_tx: broadcast::Sender<WsEnvelope>`；容量复用现有
`broadcast_channel_capacity` 配置（同一洪峰量级）。

### C3：events_handler 改造

- 解析 query：`Query<EventsParams>`，`workspace_id: Option<i64>`；
- Sync 握手：`Some(ws)` 时按 record.workspace_id 过滤 running_tasks；
  查不到 record 的异常任务**保守保留**（决策 2a）；
- 主循环：订阅 `ws_tx`：
  - 连接带 `Some(ws)`：`envelope.workspace_id == Some(ws) || envelope.workspace_id.is_none()` 才发；
  - 连接无参数：全发（兼容）；
  - 发送 `Message::text(&*envelope.json)`（Arc 共享，仅 memcpy）。

### C4：前端（useExecutionEvents.ts）

- 连接 URL 带 `?workspace_id=${selectedWorkspace}`（从全局 state 读取；null 不带参数）；
- workspace 切换：关旧连接 → 新参数重连（Sync 自动重建面板，无需额外状态迁移）；
- 重连逻辑复用现有指数退避（`getReconnectDelay`），切换时重置 attempt。

## 4. 影响面与测试

| 文件 | 改动 |
|------|------|
| `executor_service/events.rs` | +2 方法 +全变体单测 |
| `handlers/mod.rs` | WsEnvelope、forwarder、handler 过滤 |
| `main.rs` | 启动 forwarder；AppState.ws_tx 初始化 |
| `handlers/*`（AppState 构造点） | 补 ws_tx 字段（编译器找齐） |
| `frontend/src/hooks/useExecutionEvents.ts` | URL 参数 + 切换重连 |

测试：
- 后端：`workspace_id()` 13 变体矩阵；`is_feishu_direct` 判定；forwarder 过滤
  （DirectXxx 不转发/其余携带正确 workspace_id）；handler 匹配逻辑
  （Some-Some 匹配/不匹配、None 全局、无参数全推）；
- 前端：Playwright 验证（`frontend/tests/`）——连接受 workspace 参数、
  切换 workspace 后事件隔离；截图入 tmp 不入库。

## 5. 验证方案

1. `cd backend && cargo clippy --all-targets -- -D warnings`：改动文件零告警
   （全树存量告警见 093 记录，不属本批）；
2. `cd backend && cargo test`：新增单测全过，存量无回归
   （已知 git_sync 本机环境失败 1 例，与本批无关）；
3. 功能验证：`make dev` 起服务，两 workspace 各开标签页，事件按 workspace 隔离；
   飞书推送路径不受影响（DirectXxx 仍走原 channel）；
4. 前端 Playwright：WS 连接 URL 断言 + 切换 workspace 后事件归属断言。

## 6. 安全反思

- **收益**：消除跨 workspace 的任务标题/日志流可见性（现状的轻度租户泄露）；
- 无 workspace 参数连接的全推是**刻意保留的兼容口**（本服务无鉴权体系，
  单用户本地工具定位，不构成新暴露面）；
- forwarder 序列化失败（理论不可达）时跳过该事件并 error 日志，不 panic；
- 无 SQL/权限/文件路径新增面。

## 7. 已知限制 / 后续候选

- `Arc<str>` → `Bytes` 彻底零拷贝（省发送侧 memcpy）：留待实测有 CPU 压力再做；
- Sync 握手的 running_tasks 过滤依赖 execution_record 反查（TaskInfo 无 workspace 字段），
  如需根除可在 TaskInfo 补 workspace_id（改动面扩到注册点，本批不做）；
- ReviewStatusChanged 等全局事件若未来带 workspace 语义，补字段后移出 None 组。
