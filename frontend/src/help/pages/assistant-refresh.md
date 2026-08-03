# 刷新智能助手列表

## 功能位置
智能助手页 → 页面右上角「刷新」按钮（`EyeOutlined`）

## 数据流图（前端 → 后端）

```mermaid
flowchart LR
  User["点击刷新"] --> handleRefresh["handleRefresh"]
  handleRefresh --> loadData["loadData()"]
  loadData -->|"Promise.all"| API1["db.getAgentBots<br>GET /api/v1/agent-bots"]
  loadData -->|"Promise.all"| API2["db.getProjectDirectories<br>GET /api/v1/project-directories"]
  API1 --> setBots["setBots(botList)"]
  API2 --> setWorkspaces["setWorkspaces(workspaceList)"]
```

## 调用关系链路图

```mermaid
flowchart TD
  Page["AssistantManagementPage.tsx<br>AssistantManagementPage()"] --> loadData["loadData()<br>useCallback"]
  loadData --> setLoading["setLoading(true)"]
  loadData --> PromiseAll["Promise.all<br>getAgentBots + getProjectDirectories"]
  PromiseAll --> setBots["setBots"]
  PromiseAll --> setWorkspaces["setWorkspaces"]
  loadData --> setLoadingFalse["setLoading(false)"]
  Page --> RefreshBtn["刷新 Button<br>onClick=handleRefresh"]
  RefreshBtn --> loadData
  Page --> useEffect["useEffect mount loadData"]
```

## 数据结构图

```mermaid
classDiagram
  class AgentBot {
    id: number
    bot_type: string
    bot_name: string
    app_id: string
    bot_open_id: string
    owner_open_id: string
    enabled: boolean
    config: string
    created_at: string
    workspace_id: number
  }
  class ProjectDirectory {
    id: number
    path: string
    name: string|null
  }
```

## 数据变更图

```mermaid
stateDiagram-v2
  [*] --> Loading: useEffect mount loadData
  Loading --> Loaded: Promise.all 完成
  Loaded --> Loading: 点击刷新 handleRefresh
  Loaded --> Loading: 其他回调 loadData（toggle/delete/bind）
```

## 开发指导
- **前端入口**：`frontend/src/components/assistant-management/AssistantManagementPage.tsx` 的 `loadData` / `handleRefresh` 回调
- **后端入口**：`backend/src/handlers/agent_bot.rs` 处理 `GET /api/v1/agent-bots`；`backend/src/handlers/project_directory.rs` 处理 `GET /api/v1/project-directories`
- **注意**：`loadData` 用 `Promise.all` 并发拉取 bots 和 workspaces，任何回调（toggle/delete/bind/configChanged）都会触发整页 loadData 刷新
- **扩展**：新增智能助手平台类型时 `AgentBot.bot_type` 追加值，列表渲染的 `Tag color` 映射需同步更新
