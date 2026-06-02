# 飞书消息集成方案

## 一、目标

实现飞书机器人与 ntd 系统的双向消息通信。**Phase 1 必须交付以下三个功能**：

### 功能 1：单聊 — 所有消息均处理

- 用户私聊机器人发送的**每条消息**都会被接收并回复
- SDK 配置 `allowed_users: ["*"]`，接受所有用户私聊
- 回复内容：echo 原文 + 后续扩展

### 功能 2：群聊 — 仅处理 @机器人 的消息

- 群内普通消息不处理，只有 **@机器人** 的消息才响应
- SDK 配置 `group_require_mention: true` 自动过滤
- 通过 `msg.mentioned_open_ids` 判断是否被 @
- **过滤自身消息**：群内可能有多方发言（包括 bot 自己），必须排除 bot 发出的消息，避免形成回环（bot 回复 → 收到自己的消息 → 再回复 → 无限循环）

### 功能 3：`/sethome` 命令 — 设置默认回复目标

- 用户发送 `/sethome` 设置自己的默认回复目标
- 单聊中发 `/sethome` → 以该用户 open_id 为回复目标
- 群聊中发 `/sethome` → 以该群 chat_id 为回复目标
- 设置成功后回复确认消息

## 二、技术选型

### 使用 `clawrs-feishu` SDK

基于 [ZClaw-Channel-Feishu](https://github.com/AgenticWeb4/ZClaw-Channel-Feishu) Rust SDK，采用 **WebSocket 长连接**（非 Webhook）接收消息。

核心优势：
- **无需公网回调 URL**：WebSocket 主动连接飞书，不需要 webhook 端点
- **内置消息过滤**：`group_require_mention` 自动过滤群消息，只保留 @机器人的
- **内置访问控制**：DM 白名单 / 群组白名单
- **自动重连**：指数退避重连（最多 10 次）
- **ID 自动推断**：根据前缀 `oc_`/`ou_` 自动选择 receive_id_type

### 核心接口

```rust
// Channel trait — SDK 核心
pub trait Channel: Send + Sync {
    fn name(&self) -> &str;
    async fn send(&self, message: &str, recipient: &str) -> anyhow::Result<()>;
    async fn listen(&self, tx: mpsc::Sender<ChannelMessage>) -> anyhow::Result<()>;
    async fn health_check(&self) -> bool;
}

// ChannelMessage — 收到的消息
pub struct ChannelMessage {
    pub id: String,                      // 消息 ID
    pub sender: String,                  // 发送者 open_id
    pub content: String,                 // 消息文本内容（已解码）
    pub channel: String,                 // chat_id
    pub timestamp: u64,
    pub chat_type: Option<String>,       // "p2p" 或 "group"
    pub mentioned_open_ids: Vec<String>, // 被 @ 的 open_id 列表
}
```

## 三、系统架构

```
飞书服务器
  │
  │ WebSocket 长连接 (SDK 自动管理)
  ▼
┌─────────────────────────────────────────────┐
│              clawrs-feishu SDK              │
│  ┌────────────────────────────────────┐     │
│  │ FeishuChannelService               │     │
│  │  - listen() → mpsc::Sender         │     │
│  │  - send(msg, recipient)            │     │
│  │  - 自动重连 / @过滤 / 访问控制     │     │
│  └──────────────┬─────────────────────┘     │
└─────────────────┼───────────────────────────┘
                  │ mpsc::channel<ChannelMessage>
                  ▼
┌─────────────────────────────────────────────┐
│            ntd 后端 (Axum)                  │
│                                             │
│  ┌──────────────────┐                       │
│  │ FeishuListener   │ 启动时创建 channel     │
│  │ (后台 tokio task) │ 循环接收消息          │
│  └──────┬───────────┘                       │
│         │                                   │
│  ┌──────▼───────────┐  ┌────────────────┐  │
│  │ Command Router   │  │ Message Store  │  │
│  │ (/sethome 等)    │  │ (DB 记录)      │  │
│  └──────┬───────────┘  └────────────────┘  │
│         │                                   │
│  ┌──────▼───────────┐                      │
│  │ Reply Handler    │                      │
│  │ channel.send()   │                      │
│  └──────────────────┘                      │
│                                             │
│  ┌──────────────────────────────────────┐   │
│  │           SQLite Database             │   │
│  │  agent_bots / feishu_homes /         │   │
│  │  feishu_messages                     │   │
│  └──────────────────────────────────────┘   │
└─────────────────────────────────────────────┘
```

## 四、数据库变更

### 4.1 agent_bots 表 — 新增配置字段

```sql
ALTER TABLE agent_bots ADD COLUMN encrypt_key TEXT;
ALTER TABLE agent_bots ADD COLUMN verification_token TEXT;
-- 运行时配置（前端 Settings 页面可操作）
ALTER TABLE agent_bots ADD COLUMN config TEXT DEFAULT '{}';
```

`config` 字段存储 JSON，包含以下配置项：

```jsonc
{
  "group_require_mention": true,   // 群聊是否仅处理 @机器人（开关）
  "dm_enabled": true,             // 是否接收单聊消息（开关）
  "group_enabled": true,          // 是否接收群聊消息（开关）
  "echo_reply": true              // 是否 echo 回复收到的消息（开关）
}
```

默认值：所有开关均为 `true`（群聊默认仅 @、单聊默认处理、默认 echo）。

### 4.2 新增 feishu_homes 表

```sql
CREATE TABLE IF NOT EXISTS feishu_homes (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    bot_id INTEGER NOT NULL,
    user_open_id TEXT NOT NULL,
    chat_id TEXT,
    receive_id TEXT NOT NULL,
    receive_id_type TEXT NOT NULL,  -- 'chat' 或 'open_id'
    created_at TEXT,
    updated_at TEXT,
    FOREIGN KEY (bot_id) REFERENCES agent_bots(id),
    UNIQUE(bot_id, user_open_id)
);
```

### 4.3 新增 feishu_messages 表

```sql
CREATE TABLE IF NOT EXISTS feishu_messages (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    bot_id INTEGER NOT NULL,
    message_id TEXT NOT NULL UNIQUE,
    chat_id TEXT NOT NULL,
    chat_type TEXT NOT NULL,
    sender_open_id TEXT NOT NULL,
    content TEXT,
    msg_type TEXT NOT NULL DEFAULT 'text',
    is_mention BOOLEAN DEFAULT 0,
    processed BOOLEAN DEFAULT 0,
    created_at TEXT,
    FOREIGN KEY (bot_id) REFERENCES agent_bots(id)
);
```

## 五、后端模块设计

### 5.1 文件结构

```
backend/src/
├── services/
│   ├── mod.rs                (新增)
│   └── feishu_listener.rs    (新增，核心：启动监听 + 消息处理)
├── db/
│   ├── feishu_home.rs        (新增)
│   └── feishu_message.rs     (新增)
├── handlers/
│   └── agent_bot.rs          (已有，添加启动/停止监听逻辑)
└── models/
    └── mod.rs                (扩展)
```

### 5.2 `services/feishu_listener.rs` — 核心服务

```rust
use clawrs_feishu::{create_channel, Channel, ChannelMessage, FeishuConfig, FeishuDomain, FeishuConnectionMode};
use tokio::sync::mpsc;

pub struct FeishuListener {
    db: Arc<Database>,
    // bot_id → Channel
    channels: Arc<DashMap<i64, Arc<dyn Channel>>>,
}

impl FeishuListener {
    /// 为一个已绑定的 bot 启动 WebSocket 监听
    pub async fn start_bot(&self, bot: &AgentBot) -> Result<()> {
        let domain = match bot.domain.as_deref() {
            Some("lark") => FeishuDomain::Lark,
            _ => FeishuDomain::Feishu,
        };
        // 从 bot.config JSON 读取用户配置
        let bot_config: BotConfig = serde_json::from_str(&bot.config)
            .unwrap_or_default();

        let config = FeishuConfig {
            app_id: bot.app_id.clone().into(),
            app_secret: bot.app_secret.clone().into(),
            domain,
            connection_mode: FeishuConnectionMode::WebSocket,
            allowed_users: vec!["*".into()],
            group_require_mention: bot_config.group_require_mention,
            ..Default::default()
        };

        let channel = create_channel(config);
        let (tx, mut rx) = mpsc::channel::<ChannelMessage>(256);

        let ch = channel.clone();
        let bot_id = bot.id;
        tokio::spawn(async move {
            if let Err(e) = ch.listen(tx).await {
                tracing::error!("feishu listener error (bot {}): {e}", bot_id);
            }
        });

        self.channels.insert(bot.id, channel);

        // 消息处理循环
        let db = self.db.clone();
        tokio::spawn(async move {
            while let Some(msg) = rx.recv().await {
                Self::handle_message(&db, bot_id, &msg).await;
            }
        });

        Ok(())
    }

    async fn handle_message(channel: &dyn Channel, db: &Database, bot_id: i64, bot_config: &BotConfig, bot_open_id: &str, msg: &ChannelMessage) {
        // 0. 过滤自身消息 — 防止回环
        if msg.sender == bot_open_id {
            return;
        }

        // 1. 存储消息
        db.save_feishu_message(bot_id, msg).await.ok();

        let content = msg.content.trim();

        // 2. 功能3: /sethome 命令
        if content == "/sethome" {
            Self::handle_sethome(channel, db, bot_id, msg).await;
            return;
        }

        // 3. 功能1: 单聊 — 检查 dm_enabled 开关
        if msg.chat_type.as_deref() == Some("p2p") {
            if !bot_config.dm_enabled { return; }
            if bot_config.echo_reply {
                channel.send(&format!("收到：{}", content), &msg.channel).await.ok();
            }
            return;
        }

        // 4. 功能2: 群聊 — 检查 group_enabled 开关
        //    SDK 已按 group_require_mention 自动过滤，
        //    到达这里的群消息一定是 @了机器人的（如果开了该选项）
        if msg.chat_type.as_deref() == Some("group") {
            if !bot_config.group_enabled { return; }
            if bot_config.echo_reply {
                channel.send(&format!("收到：{}", content), &msg.channel).await.ok();
            }
        }
    }

    async fn handle_sethome(channel: &dyn Channel, db: &Database, bot_id: i64, msg: &ChannelMessage) {
        let (receive_id, receive_id_type, chat_id) = match msg.chat_type.as_deref() {
            Some("p2p") => (msg.sender.clone(), "open_id", None),
            Some("group") | _ => (msg.channel.clone(), "chat", Some(msg.channel.clone())),
        };

        db.set_feishu_home(bot_id, &msg.sender, chat_id.as_deref(), &receive_id, receive_id_type)
            .await
            .ok();

        // 回复确认
        let reply = match msg.chat_type.as_deref() {
            Some("p2p") => "已设置 Home ✅ 回复目标：单聊".to_string(),
            _ => "已设置 Home ✅ 回复目标：本群".to_string(),
        };
        channel.send(&reply, &msg.channel).await.ok();
    }
}
```

### 5.3 启动流程

在 `main.rs` 或 `AppState` 初始化时：

```rust
// 1. 从数据库加载所有 enabled 的飞书 bot
let bots = db.get_agent_bots().await?;

// 2. 为每个 bot 启动 FeishuListener
let listener = FeishuListener::new(db.clone());
for bot in bots.iter().filter(|b| b.bot_type == "feishu" && b.enabled) {
    if let Err(e) = listener.start_bot(bot).await {
        tracing::error!("failed to start feishu bot {}: {e}", bot.id);
    }
}
```

### 5.4 `/sethome` 命令逻辑

| 场景 | receive_id | receive_id_type | chat_id |
|------|-----------|-----------------|---------|
| 单聊发送 `/sethome` | 用户的 open_id | `open_id` | NULL |
| 群聊发送 `/sethome` | 群的 chat_id | `chat` | 群的 chat_id |

执行后回复：`已设置 Home ✅`

## 六、消息处理流程

```
clawrs-feishu SDK (WebSocket)
  │
  │ ChannelMessage 通过 mpsc::channel 推送
  ▼
handle_message()
  │
  ├─ sender == bot_open_id ?
  │    └─ 是 → 丢弃（防止回环）
  │
  ├─ 存储到 feishu_messages 表
  │
  ├─ content == "/sethome" ?
  │    └─ 是 → 功能3: sethome → 记录回复目标 → 回复确认
  │
  ├─ chat_type == "p2p" ?
  │    └─ 是 → 功能1: 单聊 → echo 回复
  │
  └─ chat_type == "group" ?
       └─ 是 → 功能2: 群聊 @机器人 → echo 回复
               （SDK 已过滤，到达这里一定 @了机器人）
```

SDK 内置的过滤链（在 `listen()` 内部自动执行）：
1. **DM 策略**：`allowed_users: ["*"]` → 接受所有私聊
2. **群聊策略**：`group_require_mention: true` → 群消息仅 @机器人 时转发
3. **访问控制**：默认开放所有群

## 七、新增依赖

```toml
# Cargo.toml
[dependencies]
clawrs-feishu = { git = "https://github.com/AgenticWeb4/ZClaw-Channel-Feishu.git" }
dashmap = "6"  # 并发 HashMap，用于管理多个 bot 的 channel
```

或者如果不想引入 dashmap，用 `tokio::sync::RwLock<HashMap>` 替代。

## 八、实施步骤

### Phase 1：基础收发 + 前端配置（本次实施）

| # | 任务 | 验证方式 |
|---|------|----------|
| 1 | 添加 `clawrs-feishu` 依赖，确保编译通过 | `cargo build` 成功 |
| 2 | 数据库 schema 变更（config 字段 + 2 张新表） | 启动无报错，表存在 |
| 3 | DB 层：feishu_home / feishu_message CRUD + config 读写 | 编译通过 |
| 4 | **功能1** 单聊：接收私聊消息并回复 | 私聊机器人 → 收到 echo 回复 |
| 5 | **功能2** 群聊：仅 @机器人 时接收并回复 | 群内 @机器人 → 收到回复；不 @ → 无回复 |
| 6 | **功能3** `/sethome` 命令实现 | 发送 /sethome → 数据库记录正确，收到确认回复 |
| 7 | 服务启动时自动加载已绑定的 bot | 重启后自动连上飞书 |
| 8 | **前端** Settings 消息页 bot 卡片增加配置开关 | 开关可切换，切换后立即生效 |

### Phase 2：前端增强（后续）

| # | 任务 |
|---|------|
| 1 | Settings 页面显示 WebSocket 监听状态（在线/离线） |
| 2 | 消息记录查看页面 |
| 3 | `/sethome` 管理界面 |

### Phase 3：Todo 联动（后续）

| # | 任务 |
|---|------|
| 1 | 通过飞书命令创建/查询/完成 todo |
| 2 | 支持更多消息类型（图片、文件、卡片） |
| 3 | 群内其他平台消息监听与捕获 |

## 九、前端配置 UI

在 Settings → 消息页 → 每个 bot 卡片下方增加配置开关区域：

```
┌─────────────────────────────────────────────┐
│  [飞]  Bot 名称         [已启用] [删除]       │
│        App ID: cli_xxx                       │
│        平台: 飞书                             │
│        绑定时间: 2026-05-06                   │
│  ─────────────────────────────────────────   │
│  消息配置                                     │
│  ┌─────────────────────────────────────┐     │
│  │ 接收单聊消息     [=========] ON     │     │
│  │ 接收群聊消息     [=========] ON     │     │
│  │ 群聊仅处理@      [=========] ON     │     │
│  │ Echo 回复        [=========] ON     │     │
│  └─────────────────────────────────────┘     │
└─────────────────────────────────────────────┘
```

配置项说明：

| 开关 | 对应字段 | 默认 | 说明 |
|------|---------|------|------|
| 接收单聊消息 | `dm_enabled` | ON | 关闭后忽略所有私聊消息 |
| 接收群聊消息 | `group_enabled` | ON | 关闭后忽略所有群聊消息 |
| 群聊仅处理@ | `group_require_mention` | ON | 关闭后群内所有消息都响应 |
| Echo 回复 | `echo_reply` | ON | 关闭后只存库不回复 |

**交互**：切换开关后立即调用 `PUT /api/agent-bots/{id}/config` 保存到数据库，后端更新 config JSON。若 `group_require_mention` 变更，需重启该 bot 的 WebSocket 监听（因为该参数在 SDK `FeishuConfig` 初始化时决定）。

## 十、安全考虑

1. **无需暴露端口**：WebSocket 出站连接，不需要公网入口
2. **Token 安全**：app_secret 不通过 API 返回（已实现）
3. **SDK 内置过滤**：@mention 检测、DM/群组访问控制
4. **消息存储**：仅存储必要字段，content 长度限制
