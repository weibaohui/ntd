# Rust 版 cc-connect 移植设计稿 —— ntd-connect

> 作者：Claude（MiniMax-M3）
> 日期：2026-06-28
> 状态：调研完成，待用户拍板
> 参考实现：`/tmp/zeroclaw-study/cc-connect`（chenhg5/cc-connect，Go，16613+ 行 `core/engine.go`）

---

## 1. 目标 & v1 范围

把 cc-connect 的「core 中立接口层」移植成 Rust crate `ntd-connect`，作为 nothing-todo 后端的子 workspace 成员，承担：

- **Channel 抽象**：飞书/钉钉/微信/TG/Slack 等多 IM 入口
- **Agent 抽象**：Claude Code / Codex / Hermes 等多 executor
- **Dispatch 抽象**：per-session lock + busy queue + watermark
- **共享基础设施**：HTTP client、dedup、rate limit

**v1 交付（Rust crate）**：
- `ntd-connect` traits：`Channel`、`Agent`、`AgentSession`、`TypingIndicator`、`MessageHandler`
- `ntd-connect` runtime：`Dispatcher`、`SessionManager`、`Dedup`、`RateLimiter`、`SharedHttpClient`
- 一个 Platform 实现：`ntd-connect/src/platform/feishu.rs`（替代现有 `services/feishu_listener.rs`）
- 一个 Agent 实现：`ntd-connect/src/agent/claude_code.rs`（替代现有调用 Claude Code 的代码）

**v1 不做**：permission hook 引擎、多 workspace 路由、provider 切换（ModelSwitcher 等可选 trait 留 stub）。

---

## 2. Workspace 结构

```toml
# nothing-todo/Cargo.toml (workspace)
[workspace]
members = ["backend", "ntd-connect"]

[workspace.dependencies]
tokio = { version = "1", features = ["full"] }
async-trait = "0.1"
reqwest = { version = "0.12", features = ["json", "stream"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
dashmap = "6"
parking_lot = "0.12"
thiserror = "1"
tracing = "0.1"
uuid = { version = "1", features = ["v4"] }
lru = "0.12"
```

```
ntd-connect/
├── Cargo.toml
├── src/
│   ├── lib.rs                  # 重新导出
│   ├── error.rs                # Error / Result
│   ├── types.rs                # Message / Event / ReplyTarget / SessionKey
│   ├── channel.rs              # Channel trait + MessageHandler + 可选 trait
│   ├── agent.rs                # Agent / AgentSession / Event
│   ├── typing.rs               # TypingIndicator trait + StopToken
│   ├── dispatcher.rs           # Engine 主循环
│   ├── session.rs              # SessionManager + SessionState
│   ├── dedup.rs                # LRU dedup
│   ├── ratelimit.rs            # Token bucket
│   ├── http.rs                 # SharedHttpClient
│   ├── platform/
│   │   ├── mod.rs
│   │   └── feishu.rs           # FeishuPlatform（替代 feishu_listener.rs）
│   └── agent_impl/
│       ├── mod.rs
│       └── claude_code.rs      # ClaudeCodeAgent（进程 spawn + JSON-line stream）
└── tests/
    ├── dispatcher_burst.rs     # 100 条 burst 集成测试
    └── platform_feishu_mock.rs # Mock channel 跑 dispatcher
```

---

## 3. Trait 设计

### 3.1 `Channel`（Platform）

对应 cc-connect `core/interfaces.go:10-16`。

```rust
#[async_trait]
pub trait Channel: Send + Sync {
    fn name(&self) -> &'static str;

    /// 启动长连接，handler 在收到入站消息时被调用。
    /// 返回成功意味着 handshake 完成；handler 可在 await 中持续触发。
    async fn start(&self, handler: Arc<dyn MessageHandler>) -> Result<()>;

    /// 回复到某个具体消息（threaded reply）。
    async fn reply(&self, ctx: &ReplyContext, target: ReplyTarget, content: OutgoingContent) -> Result<()>;

    /// 主动发一条新消息（非 reply）。
    async fn send(&self, ctx: &ReplyContext, target: ReplyTarget, content: OutgoingContent) -> Result<()>;

    async fn stop(&self) -> Result<()>;
}
```

**关键决策**：
- `handler: Arc<dyn MessageHandler>` 而非 `&dyn MessageHandler`：handler 通常是 `Dispatcher`，需要 `Arc` 跨 task 共享。
- `ReplyTarget` 是 `enum`，**不是** `Box<dyn Any>`（避开 Go 的 type erasure）。每个 platform variant 携带自己定位回复所需的全部字段。
- `OutgoingContent` 是 enum：`Text(String)` / `Markdown(String)` / `Image(...)` / `Card(...)` / `File(...)`，由 Channel 实现决定怎么降级到本平台格式。

### 3.2 `MessageHandler`

```rust
#[async_trait]
pub trait MessageHandler: Send + Sync {
    async fn on_message(&self, channel: Arc<dyn Channel>, msg: IncomingMessage) -> Result<()>;
}
```

Dispatcher 实现这个 trait。

### 3.3 `IncomingMessage`

```rust
pub struct IncomingMessage {
    pub platform: PlatformKind,           // 哪个 channel
    pub session_key: SessionKey,          // platform 注入的 key
    pub sender: SenderId,
    pub content: IncomingContent,         // Text/Markdown/Image/File/Audio
    pub reply_target: ReplyTarget,        // 用于 reply
    pub timestamp_ms: i64,
    pub raw_message_id: String,           // 用于 dedup
}

pub enum PlatformKind { Feishu, Dingtalk, Wechat, Telegram, Slack, /* ... */ }
pub struct SenderId(pub String);
pub struct SessionKey(pub String);
```

### 3.4 `ReplyTarget`

```rust
pub enum ReplyTarget {
    Feishu { chat_id: String, message_id: Option<String>, chat_type: FeishuChatType },
    // v1 只实现 Feishu；后续加变体即可
}

pub enum FeishuChatType { P2p, Group }
```

**为什么不 Box<dyn Any>**：编译期类型检查更安全，dispatcher 调 `channel.reply(..., Feishu{...})` 编译器能检查 Channel 实现是否匹配。

### 3.5 `TypingIndicator`（可选 trait）

对应 cc-connect `core/interfaces.go:246-248`。

```rust
#[async_trait]
pub trait TypingIndicator: Send + Sync {
    /// 返回一个 stop token，调用方负责 drop 时停止 typing
    /// 飞书用 👀 reaction 实现；其他平台可以是真 typing API
    async fn start_typing(&self, ctx: &ReplyContext, target: &ReplyTarget) -> Result<TypingGuard>;
}

pub struct TypingGuard {
    stop_fn: Box<dyn FnOnce() -> BoxFuture<'static, ()> + Send>,
}

impl TypingGuard {
    pub async fn stop(self) { (self.stop_fn)().await }
}
```

Dispatcher 用 `if let Some(ti) = channel.as_typing_indicator() { ti.start_typing(...).await }` 模式触发——Rust 等价于 downcast 或 sealed trait。

**Rust 实现选择**：用 `Any` 风格 dynamic dispatch（`Arc<dyn Channel>` 内部同时实现 Channel + 可选 TypingIndicator），通过 enum dispatch 模式：

```rust
// 在 dispatcher 里
match channel.concrete_kind() {
    ChannelKind::Feishu(feishu) => feishu.start_typing(...).await,
    // ...
}
```

或者更简单：FeishuPlatform 直接实现 TypingIndicator trait，dispatcher 用 `Arc::clone` 拿到两份 trait object。**待 v1 实现时定**。

### 3.6 `Agent`

对应 cc-connect `core/interfaces.go:382-390`。

```rust
#[async_trait]
pub trait Agent: Send + Sync {
    fn name(&self) -> &'static str;

    /// session_id = None 表示新 session；Some(sid) 表示 resume
    async fn start_session(&self, ctx: &AgentContext, session_id: Option<&str>)
        -> Result<Box<dyn AgentSession>>;

    async fn list_sessions(&self, ctx: &AgentContext) -> Result<Vec<AgentSessionInfo>>;
    async fn stop(&self) -> Result<()>;
}
```

### 3.7 `AgentSession`

对应 cc-connect `core/interfaces.go:393-406`。

```rust
#[async_trait]
pub trait AgentSession: Send {
    async fn send(&self, prompt: &str, attachments: &[Attachment]) -> Result<()>;
    async fn respond_permission(&self, request_id: &str, result: PermissionResult) -> Result<()>;

    /// 持续打开的只读事件流。channel close 意味着 agent 进程退出。
    /// 调用方 take 后独占——见 cc-connect engine.go:3648-3657 ownership 转移模式。
    fn take_events(&mut self) -> Option<mpsc::Receiver<Event>>;

    fn session_id(&self) -> &str;
    fn alive(&self) -> bool;
    async fn close(&self) -> Result<()>;
}
```

### 3.8 `Event`

```rust
pub enum Event {
    Text(String),
    ToolUse { name: String, args: serde_json::Value },
    Result { usage: Usage, duration_ms: u64 },
    PermissionRequest { request_id: String, tool: String, args: serde_json::Value },
    Error(String),
    Closed,
}
```

---

## 4. Dispatcher 消息生命周期

对应 cc-connect `core/engine.go:2300-3100, 3551-3699, 5878-5937`。

### 4.1 调用栈（伪代码）

```rust
// Channel.on_message → Dispatcher.handle_message
async fn handle_message(&self, channel: Arc<dyn Channel>, msg: IncomingMessage) -> Result<()> {
    // 0. dedup：丢弃重复 message_id
    if !self.dedup.check_and_set(&msg.raw_message_id) { return Ok(()); }

    // 1. 派生 session key（workspace + platform 注入的 key）
    let key = SessionKey::derive(&msg);

    // 2. session manager: get_or_create
    let session = self.sessions.get_or_create(key);

    // 3. try lock
    if !session.try_lock() {
        // busy: 走 watermark + 入队
        if session.is_stale(&msg) {
            debug!("stale message dropped: {:?}", msg.raw_message_id);
            return Ok(());
        }
        if session.pending_len() >= MAX_QUEUED {
            warn!("queue full for session {}", key);
            return Ok(());
        }
        session.enqueue(msg);
        return Ok(());
    }

    // 4. 抢到锁 → spawn worker（仍受全局 dispatcher JoinSet bound）
    let permit = self.dispatch_sem.clone().acquire_owned().await?;
    self.workers.spawn(worker(session, channel, msg, permit));
    Ok(())
}

async fn worker(session: Arc<SessionState>, channel: Arc<dyn Channel>, msg: IncomingMessage, _permit: OwnedSemaphorePermit) {
    // 1. 创建/复用 AgentSession
    let agent_session = session.get_or_spawn_agent(&self.agent).await?;

    // 2. typing start
    let typing = if let Some(ti) = channel.as_typing() {
        Some(ti.start_typing(&ctx, &msg.reply_target).await?)
    } else { None };

    // 3. send prompt（spawn 后台，权限请求 event 可能阻塞 Send）
    let send_fut = agent_session.send(&prompt, &[]);
    let mut events = agent_session.take_events().unwrap();

    // 4. event loop
    let mut drain_remaining = true;
    while let Some(event) = events.recv().await {
        match event {
            Event::Text(s) => throttle_send(&channel, &msg.reply_target, &s).await,
            Event::PermissionRequest { request_id, .. } => {
                // cc-connect engine.go:3637-3646: stop typing, 询问用户, respond
                let decision = handle_permission(&request_id).await;
                agent_session.respond_permission(&request_id, decision).await?;
            }
            Event::Result { .. } => {
                // turn done → drain pending if any
                drain_remaining = false;
                break;
            }
            Event::Error(e) => { error!("agent error: {e}"); }
            Event::Closed => break,
            _ => {}
        }
    }
    if let Some(t) = typing { t.stop().await; }

    // 5. drain pending messages
    while let Some(queued) = session.pop_pending() {
        if session.is_stale(&queued) { continue; }
        // 复用相同处理：send + event loop
        // 简化：v1 直接结束当前 turn + 立即 drain 一次（每次 drain 也是 per-session 串行）
        ...
    }

    // 6. release lock
    session.unlock();
}
```

### 4.2 关键决策

| 决策点 | 选择 | 原因 |
|--------|------|------|
| Session lock 实现 | `AtomicBool` CAS（不用 `tokio::Mutex::try_lock`）| 简单、无 await、CAS 单指令；try_lock 在 tokio 里语义模糊 |
| Pending queue 数据结构 | `parking_lot::Mutex<VecDeque<QueuedMsg>>` | 短临界区；std Mutex 容易误用 |
| 全局并发限流 | `Arc<Semaphore>` + `JoinSet` | 避免 worker 无限 spawn |
| Watermark | `AtomicI64` × 2（current/last_completed）| CAS 更新；drain 时只看 current+completed，不看 pending |
| event channel 容量 | 64（buffered mpsc）| 与 cc-connect session.go:391 一致 |

### 4.3 Watermark 三态（关键！）

```rust
impl SessionState {
    fn watermark(&self) -> i64 {
        max(self.current_turn.load(Acquire), self.last_completed.load(Acquire))
    }
    fn is_stale(&self, msg: &IncomingMessage) -> bool {
        msg.timestamp_ms < self.watermark()
    }
    // drain 时只看 current+completed，不看 pending —— 防止晚到消息让早到消息失效
}
```

对应 cc-connect `core/engine.go:2517-2526 isQueuedUserMessageStaleForDrainLocked`。

---

## 5. SessionManager

```rust
pub struct SessionManager {
    inner: Arc<DashMap<SessionKey, Arc<SessionState>>>,
    lru: Arc<Mutex<LruCache<SessionKey, ()>>>,  // 容量 1024
}

impl SessionManager {
    pub fn get_or_create(&self, key: SessionKey) -> Arc<SessionState> {
        if let Some(s) = self.inner.get(&key) { return s.clone(); }
        let state = Arc::new(SessionState::new());
        self.inner.insert(key.clone(), state.clone());
        self.lru.lock().put(key, ());
        // LRU 满了：驱逐
        while self.lru.lock().len() > MAX_SESSIONS {
            if let Some((evicted_key, _)) = self.lru.lock().pop_lru() {
                self.inner.remove(&evicted_key);
            }
        }
        state
    }
}
```

**为什么 LRU 上限**：防止一个恶意 bot 不断发消息生成无限 session 撑爆内存。1024 是参考值（可调）。

---

## 6. 共享基础设施

### 6.1 `SharedHttpClient`

```rust
#[derive(Clone)]
pub struct SharedHttpClient {
    inner: reqwest::Client,
}

impl SharedHttpClient {
    pub fn new() -> Self {
        let inner = reqwest::Client::builder()
            .pool_max_idle_per_host(8)
            .timeout(Duration::from_secs(30))
            .build()
            .expect("reqwest client build");
        Self { inner }
    }
    pub fn raw(&self) -> &reqwest::Client { &self.inner }
}
```

持有位置：`Dispatcher` 结构内 + `FeishuPlatform` 结构内 + `ClaudeCodeAgent` 结构内（`Arc<SharedHttpClient>`）。

### 6.2 Dedup

```rust
pub struct Dedup {
    inner: Arc<Mutex<LruCache<String, Instant>>>,
}

impl Dedup {
    pub fn check_and_set(&self, key: &str) -> bool {
        let mut cache = self.inner.lock();
        if cache.contains(key) { return false; }
        cache.put(key.to_string(), Instant::now());
        true
    }
    // 定期清理：background task 每 30s 扫一遍，删掉过 60s 的
}
```

### 6.3 RateLimiter

v1 简化：只做「每 platform 每秒最多 N 次出站」的 token bucket。cc-connect 的 `OutgoingRateLimiter` v1 不强求对齐。

```rust
pub struct RateLimiter {
    sem: Arc<Semaphore>,
    refill: Arc<Notify>,
}
```

或者直接用 `governor::Quota`：

```rust
pub type RateLimiter = Arc<governor::RateLimiter<NotKeyed, ...>>;
```

**v1 决策**：先用 `governor` crate（成熟），不自己写。

---

## 7. Platform 实现：FeishuPlatform

对应 cc-connect `platform/feishu/feishu.go`（6474 行）。**v1 只移植核心子集**：

| cc-connect 功能 | v1 处理 |
|-----------------|---------|
| WS 长连接 `listen_ws` | ✅ 移植 |
| Webhook 模式 `listen_http` | ❌ v2 |
| `startWebSocketMode` | ✅ 移植 |
| 共享 WS group | ❌ v2 |
| `addReaction` / `removeReaction` | ✅ 移植（用于 TypingIndicator） |
| `addDoneReaction` | ✅ 移植 |
| imageBatch coalesce | ❌ v2（v1 debounce 由 MessageDebounce 继续负责） |
| 多租户多 bot | ✅ 移植（v1 只支持一个 bot，多 bot v2） |

**关键映射**：
- cc-connect `addReaction(emoji)` 同步拿 reaction_id → Rust 版 `start_typing` 返回 `TypingGuard` 持有 reaction_id，`stop` 时调 `removeReaction`
- cc-connect `Platform.Start(handler)` → Rust `Channel::start(handler)`，handler 是 `Arc<dyn MessageHandler>`
- cc-connect `Platform.Reply(ctx, replyCtx any, content)` → Rust `Channel::reply(ctx, ReplyTarget::Feishu{...}, content)`

---

## 8. Agent 实现：ClaudeCodeAgent

对应 cc-connect `agent/claudecode/`（claudecode.go 1502 行 + session.go 1275 行）。**v1 只移植核心子集**：

| cc-connect 功能 | v1 处理 |
|-----------------|---------|
| `New()` / `init()` 注册 | ✅ |
| `StartSession()` spawn 进程 | ✅ |
| `stream-json` + `input-format stream-json` args | ✅ |
| `--permission-prompt-tool stdio` | ❌ v2（v1 自动 allow 所有权限） |
| `readLoop` JSON line scanner | ✅ 用 `tokio::io::BufReader::lines()` |
| `writeJSON` stdin | ✅ 用 `ChildStdin::write_all` |
| 三段式优雅关闭 | ✅（close stdin → SIGTERM group → SIGKILL group） |
| 进程组 Setpgid | ✅ 用 `CommandExt::pre_exec` |
| `--replay-user-messages` | ✅ |
| `--resume <sid>` | ✅ |
| `--continue` 哨兵 | ✅ |
| hooks runner (`cc_hooks.go`) | ❌ v2 |

**关键映射**：
- cc-connect `cmd.StdinPipe()` → Rust `ChildStdin`
- cc-connect `bufio.Scanner` 10MB buffer → Rust `BufReader::new(child_stdout)` + 大行处理
- cc-connect `cs.events chan core.Event` → Rust `mpsc::Sender<Event>`，从 `ChildStdout` read loop 写入

---

## 9. 集成到 nothing-todo

### 9.1 工作流

```
nothing-todo/
├── Cargo.toml                    # + ntd-connect member
├── backend/                      # 现有后端
│   ├── Cargo.toml                # + dep ntd-connect
│   └── src/
│       ├── main.rs               # 启动 Dispatcher
│       └── services/
│           └── feishu_listener.rs  # 删 / 瘦身为 Dispatcher 启动 stub
└── ntd-connect/                     # 新建
    ├── Cargo.toml
    └── src/...
```

### 9.2 迁移路径（用户选了「一次替代旧代码」）

1. **新建 `ntd-connect` crate**，实现 traits + feishu platform + claude_code agent + dispatcher
2. **`backend/Cargo.toml` 加 `ntd-connect` 依赖**
3. **`backend/src/main.rs` 启动 Dispatcher** 替代 `FeishuListener::start()`
4. **删除 `backend/src/services/feishu_listener.rs`**（一次 PR）
5. **保留 `backend/src/services/message_debounce.rs`**：v1 仍由 Dispatcher 调它（debounce 是 dispatcher 内的 stage）
6. **保留 `backend/src/services/feishu_history_fetcher.rs`**：history poll 不经过 dispatcher，独立后台 task
7. **保留 `backend/src/handlers/agent_bot.rs` 等 HTTP handler**：dispatcher 通过 HTTP 暴露 status/sessions 列表

### 9.3 不动的部分

- 前端 `MessagesPanel` / `BotDetailPage`：API 协议不变
- `feishu_messages` / `feishu_history_chats` 数据库表：schema 不变
- `agent_bots` / `project_directories` 等配置表：schema 不变

---

## 10. 关键技术决策（争议点，待 v1 实现时定）

| 决策点 | 候选 | 当前倾向 | 待定原因 |
|--------|------|----------|----------|
| Channel trait object 边界 | `Box<dyn Channel>` vs enum dispatch | `Arc<dyn Channel>` | 后者编译期检查但加 channel 要改 enum；前者灵活但 dyn 限制 |
| ReplyTarget 类型 | enum vs `Box<dyn Any>` | enum | enum 编译期检查，避免 downcast |
| Session lock | `AtomicBool` CAS vs `tokio::Mutex::try_lock` | `AtomicBool` | tokio Mutex try_lock 语义不直观 |
| Pending queue | `Mutex<VecDeque>` vs `mpsc::channel` | `Mutex<VecDeque>` | cc-connect 也用 slice；mpsc 容量限制不好处理 LRU 驱逐 |
| AgentSession events channel | `mpsc::channel` vs `tokio::sync::broadcast` | `mpsc` | 单消费者；broadcast 浪费 |
| Dispatcher 并发 | `JoinSet` vs `Vec<JoinHandle>` | `JoinSet` | 自动清理完成 task；Vec 需要手动 remove |
| Platform 启动 | `async fn start` | async | cc-connect Start 是 sync 但本质握手是 async |
| Rate limiter | `governor` crate vs 自己写 | `governor` | 成熟；v2 再考虑自研 |
| LRU 库 | `lru` vs `quickcache` | `lru` | API 更直观 |
| 日志格式 | `tracing` vs `log` | `tracing` | 已有 nothing-todo 用 tracing |

---

## 11. 测试策略

### 11.1 单元测试（每个 trait 文件夹）

- `tests/channel_mock.rs`：实现一个 `MockChannel`（不连真实飞书），验证 dispatcher 收到消息后正确入队/派发
- `tests/session_manager.rs`：100 个不同 SessionKey，验证 LRU 驱逐
- `tests/dedup.rs`：相同 message_id 第二次返回 false

### 11.2 集成测试（`tests/dispatcher_burst.rs`）

```rust
#[tokio::test]
async fn burst_100_messages_completes_under_5s() {
    // 启动 dispatcher + MockChannel + MockAgent（reply < 50ms）
    // burst 发 100 条消息
    // 断言总耗时 < 5s（vs 当前 50s+）
}
```

### 11.3 端到端测试（手动）

dev server + 真飞书 bot + 真 Claude Code：连发 50 条点赞，观察：
- 50 条 `收到私聊消息` 日志 < 5s 全部出现
- 50 个 👀 reaction 出现在飞书端
- 不出现飞书 API 429

---

## 12. 风险与缓解

| 风险 | 概率 | 缓解 |
|------|------|------|
| 现有飞书 bot 兼容性破坏 | 中 | v1 保留同样的 webhook URL 格式、token config 路径；DB schema 不变 |
| Trait 设计缺陷导致后期返工 | 高 | v1 只 port 一种 platform 验证；trait 抽象有问题就改 |
| 进程 spawn 兼容性问题（macOS dev / linux prod）| 低 | cc-connect 用 `proc_unix.go` + `proc_windows.go`；Rust 用 `cfg(unix)` + `cfg(windows)` |
| 现有 `message_debounce.rs` 兼容性 | 中 | v1 保留 debounce，dispatcher 内部调它；debounce 自己的并发模型不变 |
| `reqwest::Client` 单例 vs 多实例 | 低 | 用 `SharedHttpClient` 注入；测试用 `mockito` 或 `wiremock` |

---

## 13. v1 交付物清单

**代码**：
- `ntd-connect/Cargo.toml`
- `ntd-connect/src/` 全套模块（约 1500~2000 行 Rust）
- `ntd-connect/tests/` 集成测试
- `backend/Cargo.toml` + `backend/src/main.rs` 接入
- 删除 `backend/src/services/feishu_listener.rs`

**文档**：
- 每个 trait 的 rustdoc（含 usage example）
- `ntd-connect/README.md`：架构图 + 上手示例
- `ntd-connect/CHANGELOG.md`：v0.1.0

**测试**：
- 单元测试覆盖每个 trait
- 集成测试 `dispatcher_burst` 验证 100 条 burst < 5s
- 手工 e2e 验证：连发 50 条点赞

**估时**：2~3 周（一个人；agent/AI 协助可加速）

---

## 14. 不在 v1（v2+）

- permission hook 引擎（Claude Code 的 can_use_tool）
- Provider / Model 切换（Anthropic/Bedrock/Vertex/Foundry）
- CardSender / ImageSender / FileSender 可选 trait 实现
- 多 workspace 路由 + workspace 隔离
- 多 channel 并存（飞书 + 钉钉同时运行）
- 共享 WS group（多工程共享连接）
- 历史消息拉取（feishu_history_fetcher）迁到 ntd-connect
- Webhook 模式（listen_http）替代 WS
- 多租户多 bot 并存

---

## 附录 A：cc-connect 关键文件位置速查

- `core/interfaces.go:10-16` — `Platform` trait
- `core/interfaces.go:246-248` — `TypingIndicator` trait
- `core/interfaces.go:379` — `MessageHandler` 类型
- `core/interfaces.go:382-406` — `Agent` + `AgentSession` traits
- `core/engine.go:2314` — `Engine.ReceiveMessage` 入口
- `core/engine.go:2669-2947` — `handleMessage` 路由主逻辑
- `core/engine.go:2892` — `session.TryLock()` 互斥点
- `core/engine.go:3010-3077` — `queueMessageForBusySession` 入队逻辑
- `core/engine.go:3551-3699` — `processInteractiveMessageWith` turn 主循环
- `core/engine.go:3671-3674` — Send 并发模式（go + chan）
- `core/engine.go:5878-5937` — `drainPendingMessages` drain loop
- `core/session.go:43-80` — Session struct + TryLock 实现
- `core/session.go:788-804` — SessionKey 格式
- `core/dedup.go:8-44` — `MessageDedup` LRU 60s TTL
- `core/httpclient.go:9-11` — 全局 HTTPClient 单例
- `core/outgoing_ratelimit.go:27-101` — token bucket 限流
- `agent/claudecode/claudecode.go` — Agent struct + StartSession factory
- `agent/claudecode/session.go:207-453` — 进程 spawn + pipe
- `agent/claudecode/session.go:455-466` — readLoop JSON line scanner
- `agent/claudecode/session.go:1054-1066` — writeJSON stdin
- `agent/claudecode/session.go:1159-1216` — 三段式优雅关闭
- `agent/claudecode/proc_unix.go:20-28` — Setpgid

---

## 附录 B：v1 里程碑拆分

| 里程碑 | 周 | 产出 |
|--------|-----|------|
| **M1**: trait + types + http + dedup | 0.5 周 | `ntd-connect/src/{channel,agent,typing,types,http,dedup}.rs` + 单元测试 |
| **M2**: SessionManager + Dispatcher | 0.5 周 | `ntd-connect/src/{session,dispatcher}.rs` + dispatcher_burst 测试通过 |
| **M3**: FeishuPlatform | 1 周 | `ntd-connect/src/platform/feishu.rs` + dev server 启动正常 |
| **M4**: ClaudeCodeAgent | 0.5 周 | `ntd-connect/src/agent_impl/claude_code.rs` + 手动 spawn claude 进程验证 |
| **M5**: 集成 + 切换 | 0.5 周 | `backend/src/main.rs` 接入 + 删除 feishu_listener.rs + e2e 验证 |

合计：~3 周（含联调）。
