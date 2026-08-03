# 删除智能助手

## 功能位置
智能助手页 → 列表/卡片「删除」按钮（`DeleteOutlined`，danger）→ `Popconfirm`「确定删除此智能体？」

## 数据流图（前端 → 后端）

```mermaid
flowchart LR
  User["点击删除"] --> Popconfirm["Popconfirm 确认"]
  Popconfirm -->|"onConfirm"| handleDelete["handleDelete(bot)"]
  handleDelete -->|"db.deleteAgentBot(bot.id)"| API["DELETE /api/v1/agent-bots/{id}"]
  API --> loadData["loadData() 刷新"]
```

## 谑用关系链路图

```mermaid
flowchart TD
  Page["AssistantManagementPage.tsx<br>AssistantManagementPage()"] --> handleDelete["handleDelete(bot)"]
  handleDelete --> db1["db.deleteAgentBot(bot.id)"]
  db1 --> loadData["loadData()"]
  Page --> Table["AssistantListTable.tsx<br>onDelete prop"]
  Page --> Cards["AssistantListCards.tsx<br>onDelete prop"]
  Table --> Popconfirm["Popconfirm<br>title: 确定删除此智能体？"]
  Cards --> Popconfirm
  Popconfirm --> handleDelete
```

## 数据结构图

```mermaid
classDiagram
  class AgentBot {
    id: number
    bot_name: string
    bot_type: string
  }
```

## 数据变更图

```mermaid
stateDiagram-v2
  [*] --> Idle: 列表展示
  Idle → ConfirmOpen: 点击删除
  ConfirmOpen --> Idle: 取消 Popconfirm
  ConfirmOpen --> Deleting: 确认删除
  Deleting --> Idle: delete 成功 loadData
```

## 开发指导
- **前端入口**：`frontend/src/components/assistant-management/AssistantManagementPage.tsx` 的 `handleDelete` 回调；传递给 `AssistantListTable` / `AssistantListCards` 的 `onDelete` prop
- **后端入口**：`backend/src/handlers/agent_bot.rs` 处理 `DELETE /api/v1/agent-bots/{id}`，删除 bot 及关联的推送规则、白名单等
- **注意**：删除前用 `Popconfirm` 二次确认；`deleteAgentBot` 无返回体（`await api.delete`），失败时 catch 静默不弹错误
- **扩展**：如需删除前检查是否仍有运行中任务，后端 delete handler 追加前置校验并返回 409
