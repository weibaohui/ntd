# 打开配置抽屉

## 功能位置
智能助手页 → 列表/卡片「配置」按钮（`SettingOutlined`）→ `AssistantConfigDrawer` 抽屉

## 数据流图（前端 → 后端）

```mermaid
flowchart LR
  User["点击配置"] --> handleOpenConfig["handleOpenConfig(bot)"]
  handleOpenConfig --> DrawerOpen["setConfigDrawerOpen(true)"]
  DrawerOpen --> loadConfig["AssistantConfigDrawer loadConfig"]
  loadConfig -->|"Promise.all"| API1["db.getFeishuPush<br>GET /api/v1/feishu/push"]
  loadConfig -->|"Promise.all"| API2["db.getGroupWhitelist(bot.id)<br>GET /api/v1/agent-bots/{id}/whitelist"]
  Drawer --> handleSavePush["保存推送<br>db.updateFeishuPush"]
  Drawer --> handleAddWhitelist["添加白名单<br>db.addGroupWhitelist"]
  Drawer --> handleDeleteWhitelist["删白名单<br>db.deleteGroupWhitelist"]
  Drawer --> handleMoveWorkspace["切工作空间<br>db.moveBotToWorkspace"]
  Drawer --> handleSaveBotConfig["保存接收策略<br>db.updateAgentBotConfig"]
```

## 谑用关系链路图

```mermaid
flowchart TD
  Page["AssistantManagementPage.tsx<br>AssistantManagementPage()"] --> handleOpenConfig["handleOpenConfig(bot)"]
  handleOpenConfig --> setSelectedBot["setSelectedBot<br>setConfigDrawerOpen(true)"]
  Page --> AssistantConfigDrawer["AssistantConfigDrawer.tsx"]
  AssistantConfigDrawer --> useEffectOpen["useEffect open+bot → loadConfig"]
  useEffectOpen --> loadConfig["loadConfig()"]
  loadConfig --> PromiseAll["Promise.all<br>getFeishuPush + getGroupWhitelist"]
  loadConfig --> parseBotConfig["JSON.parse bot.config<br>提取接收策略开关"]
  AssistantConfigDrawer --> StrategyTab["接收策略 Tab<br>dm/group/mention/echo 开关"]
  AssistantConfigDrawer --> PushTab["推送规则 Tab<br>pushLevel/debounce"]
  AssistantConfigDrawer --> WhitelistTab["群聊白名单 Tab"]
  AssistantConfigDrawer --> WorkspaceSelect["服务工作空间 Select<br>handleMoveWorkspace"]
```

## 数据结构图

```mermaid
classDiagram
  class AgentBot {
    id: number
    bot_name: string
    bot_type: string
    app_id: string
    config: string
    workspace_id: number
    enabled: boolean
  }
  class WhitelistEntry {
    id: number
    bot_id: number
    sender_open_id: string
    sender_name: string
  }
  class BotConfig {
    dm_enabled: boolean
    group_enabled: boolean
    group_require_mention: boolean
    echo_reply: boolean
  }
```

## 数据变更图

```mermaid
stateDiagram-v2
  [*] --> Closed: 抽屉未打开
  Closed --> Loading: 点击配置 handleOpenConfig
  Loading --> Open: loadConfig 完成
  Open --> PushTab: 切换推送规则
  Open --> WhitelistTab: 切换群聊白名单
  Open --> StrategyTab: 切换接收策略
  Open --> Saving: 任一保存回调
  Saving --> Open: onChanged 刷新
  Open --> Closed: 关闭抽屉
```

## 开发指导
- **前端入口**：`frontend/src/components/assistant-management/AssistantConfigDrawer.tsx` 的 `AssistantConfigDrawer` 组件；由 `AssistantManagementPage` 的 `handleOpenConfig` 驱动
- **后端入口**：`backend/src/handlers/agent_bot.rs` 处理 whitelist / workspace-move；飞书推送规则接口见 `backend/src/handlers/` 对应的 feishu push handler
- **注意**：`bot.config` 是 JSON 字符串，接收策略开关（`dm_enabled` 等）存在其中，保存时需 `JSON.stringify` 后调 `db.updateAgentBotConfig`
- **扩展**：新增抽屉 Tab 时在 `activeTab` union 追加值并添加条件渲染分支 + 对应保存回调
