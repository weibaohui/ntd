# 019-new指令未创建新session-缺陷分析

| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| AI (Pi) | 2026-08-09 | 初始版本 |
| AI (Claude) | 2026-08-23 | 评审补充（PR #1080）：§3 标注方案 A 前提存疑、澄清 B 的错误分支语义 |

## 1. 根因（一句话）

卡片回调通路把 `chat_type` 写死为 `"card_callback"`，真实会话类型（p2p/group）在传输中丢失；
`act_new` 退而用 `msg.channel.is_empty()` 推断会话维度——但**飞书单聊也有 chat_id**，
channel 恒非空 → scope 恒判为 **Group** → 单聊的 `dm:` 维度 session 从未被清除。

## 2. 证据链

1. **存储模型**（110 设计）：`workspaces.executor_sessions` JSON 按 `{scope}:{executor}` 分键
   （`dm:claudecode` / `group:claudecode`），单聊与群聊会话隔离（`db/workspace.rs:245-280`）。

2. **执行读侧**：`handle_butler_chat` 用 `ExecutorSessionScope::from_chat_type(chat_type)`——
   单聊消息 `chat_type="p2p"` → 读 `dm:` 键（`message_debounce.rs:1424`）。

3. **文本 /new 清侧**（正确）：`handle_new` 同样走 `from_chat_type(prep.chat_type)`——
   单聊清 `dm:` 键（`feishu_slash_commands.rs:334-336`）。✅ 读写同维度，无 bug。

4. **卡片 /new 清侧**（错误）：卡片回调在 `channel.rs:311-319` 构造 ChannelMessage 时
   `chat_type: Some("card_callback")`（真实类型丢失），`channel: chat_id`（**单聊也有 chat_id**）。
   `act_new`（`feishu_card_actions.rs:202-208`）：
   ```rust
   let scope = if msg.channel.is_empty() { Dm } else { Group };  // ← channel 恒非空 → 恒 Group
   ```
   → 单聊点卡片按钮清的是 `group:` 键，`dm:` 键纹丝不动 → 下一条消息读 `dm:` 旧值 resume。❌

5. **该错误推断的来源**：注释自述「与 resolve_receive_target 同口径」——但
   `resolve_receive_target` 用 channel 推断的是**回复地址**（chat_id 回复单聊也成立），
   不是会话类型。回复口径被误当类型口径。

## 3. 修复方案选型

| 方案 | 判断 |
|------|------|
| A. `CardActionContext` 增 `chat_type: Option<String>`（serde default 零风险），channel.rs 透传真实类型进 ChannelMessage 新字段，`act_new` 用 `from_chat_type` 恢复正确 scope | ✅ 推荐：修在数据源头；若飞书 payload 无此字段则回退现状（Group），不劣化 |
| B. act_new 用 chat_id 查 `feishu_messages` 最近一条消息的 chat_type | 备选：依赖历史消息（清库/新会话无历史时失效） |
| C. 卡片 /new 直接清两个 scope | ❌ 违背 110「单聊/new 不误伤群聊」的隔离设计 |

**采用 A + B 组合兜底**：A 为主（飞书 card.action.trigger 的 context 带 chat_type 字段）；
payload 缺失时经 B（本 bot 的入站消息表持久化了每条消息的 chat_type）反查；
两者皆无（全新会话无历史）回退 Group（= 现状）。

> **2026-08-23 评审补充**：
> 1. 方案 A 前提存疑——飞书官方文档（卡片回传交互回调）列出的 card.action.trigger `context`
>    字段为 url/preview_token/open_message_id/open_chat_id，**未含 chat_type**。线上 A 很可能
>    恒取不到值，**实际修复主力是 B**（入站消息反查，不依赖 payload 字段）。A 的透传层保留
>    （向前兼容，飞书若补字段即自动生效），待真实 payload 抓包定论。
> 2. 方案 B 的错误分支语义（CodeRabbit CR-2 采纳）：B 查询**出错**（DB 故障）时不回退 Group
>    盲清，而是显式失败提示重试——读不到事实时清任何维度都是盲清。上文「回退 Group」仅指
>    查询**成功但无历史**的场景。

## 4. 影响面

- `feishu/sdk/event.rs`：CardActionContext 加一个 Option 字段（serde default，对存量事件零风险）；
- `feishu/channel.rs`：ChannelMessage 加一个 `origin_chat_type: Option<String>` 字段（全链 Option 不扰动既有构造点）；
- `feishu_card_actions.rs::act_new`：scope 推断改为「origin_chat_type → messages 表反查 → Group 兜底」；
- 测试：act_new 的 scope 推断三态（p2p/group/未知）+ 反查回退。
