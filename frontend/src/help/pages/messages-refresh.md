# 刷新消息列表

## 功能位置

消息页 → 顶部 `MessageHeader` 的「刷新」按钮（`ReloadOutlined`）

## 数据流图（前端 → 后端）

```mermaid
flowchart LR
  U[用户点击刷新按钮] --> HDR[MessageHeader onRefresh]
  HDR --> HR[handleRefresh]
  HR --> LM[loadMessages 调用]
  LM --> DBM["db.getFeishuHistoryMessages(筛选参数)"]
  DBM --> API["GET /api/v1/feishu/history-messages"]
  API --> H[get_history_messages handler]
  H --> DAO["db 查询 feishu_messages 表"]
  DAO --> RES[分页消息列表 + total]
  RES --> SM["setMessages / setMessagesTotal"]
  HR --> DS["db.getFeishuMessageStats(workspaceId)"]
  DS --> API2["GET /api/v1/feishu/message-stats"]
  API2 --> H2[get_message_stats handler]
  H2 --> DAO2["db 聚合 feishu_messages 统计"]
  DAO2 --> SS["setStats"]
```

## 调用关系链路图

```mermaid
flowchart TD
  MessagesPage --> handleRefresh
  handleRefresh --> loadMessages
  loadMessages --> db_getMsg["db.getFeishuHistoryMessages"]
  handleRefresh --> db_stats["db.getFeishuMessageStats"]
  db_getMsg --> api_get["api.get /api/v1/feishu/history-messages"]
  db_stats --> api_stats["api.get /api/v1/feishu/message-stats"]
  api_get --> unwrap["unwrap 剥 ApiResponse"]
  api_stats --> unwrap2["unwrap 剥 ApiResponse"]
  unwrap --> setMessages
  unwrap --> setMessagesTotal
  unwrap2 --> setStats
```

## 数据结构图

```mermaid
classDiagram
  class FeishuHistoryMessagesPage {
    +messages: FeishuHistoryMessage[]
    +total: number
    +page: number
    +page_size: number
  }
  class FeishuMessageStats {
    +total_messages: number
    +processed: number
    +unprocessed: number
    +triggered_todos: number
    +unique_senders: number
    +last_24h_messages: number
    +unique_chats: number
  }
  class HistoryMessagesQuery {
    +chat_id: Option_String
    +is_history: Option_bool
    +processed: Option_bool
    +chat_type: Option_String
    +keyword: Option_String
    +processed_type: Option_String
    +workspace_id: Option_i64
    +bot_id: Option_i64
    +page: Option_i64
    +page_size: Option_i64
  }
  FeishuHistoryMessagesPage --> FeishuHistoryMessage
```

## 数据变更图

```mermaid
stateDiagram-v2
  [*] --> Idle: 页面已加载
  Idle --> Loading: 点击刷新按钮
  Loading --> Refreshing: loadMessages + getFeishuMessageStats 并发
  Refreshing --> Idle: 两个请求都完成（finally setMessagesLoading false）
  Refreshing --> Error: 请求异常（catch 静默吞错）
  Error --> Idle: finally 重置 loading
```

## 开发指导

- **前端入口**：`frontend/src/components/MessagesPage.tsx` 的 `handleRefresh` 函数
- **后端入口**：`backend/src/handlers/feishu_history.rs` 的 `get_history_messages` 和 `get_message_stats` handler
- **注意**：`loadMessages` 的依赖数组包含所有筛选状态（selectedChatId / isHistory / processedFilter / chatTypeFilter / processedTypeFilter / messagesPage / messagesPageSize / activeBotId / debouncedSearch），任一变化都会自动触发重新拉取；手动刷新只是额外同时重拉统计，不要在 `handleRefresh` 中重复调用 `loadMessages` 已经覆盖的逻辑
- **扩展**：若需要刷新时清空缓存（如 `runDataCache`），在 `handleRefresh` 内追加清空逻辑后调用 `loadMessages`；新增统计维度时在 `FeishuMessageStats` 接口和后端 `MessageStatsParams` / SQL 聚合中同步添加
