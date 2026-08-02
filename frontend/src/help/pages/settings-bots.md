# 智能助手页

> 总览

## 页面简介

智能助手页（AssistantManagementPage）是前端导航中的独立菜单项，统一管理所有已绑定的智能助手（目前支持飞书智能体）。页面提供刷新、绑定新智能助手两个入口，并以表格（桌面）或卡片（移动端）形式展示智能助手列表。每个智能助手可配置推送规则、群聊白名单、接收策略，以及切换服务工作空间、启用/停用、删除。

## 页面级数据流总图

```mermaid
flowchart LR
  User["用户"] --> Page["AssistantManagementPage.tsx"]
  Page -->|"loadData<br>Promise.all"| API1["db.getAgentBots<br>GET /api/v1/agent-bots"]
  Page -->|"loadData<br>Promise.all"| API2["db.getProjectDirectories<br>GET /api/v1/project-directories"]
  Page -->|"handleStartBind"| BindFlow["feishuInit→feishuBegin→feishuPollSSE"]
  BindFlow -->|"POST"| API3["POST /api/v1/agent-bots/feishu/init"]
  BindFlow -->|"POST"| API4["POST /api/v1/agent-bots/feishu/begin"]
  BindFlow -->|"SSE"| API5["GET /api/v1/agent-bots/feishu/poll (EventSource)"]
  Page --> handleToggleEnabled["handleToggleEnabled"]
  handleToggleEnabled -->|"PUT"| API6["PUT /api/v1/agent-bots/{id}/config"]
  Page --> handleDelete["handleDelete"]
  handleDelete -->|"DELETE"| API7["DELETE /api/v1/agent-bots/{id}"]
  Page --> AssistantConfigDrawer["AssistantConfigDrawer"]
```

## 功能点索引

- [assistant-refresh](assistant-refresh.md) — 刷新智能助手列表
- [assistant-bind](assistant-bind.md) — 绑定飞书智能助手（二维码 + SSE 轮询）
- [assistant-open-config](assistant-open-config.md) — 打开配置抽屉（推送/白名单/接收策略）
- [assistant-toggle-enabled](assistant-toggle-enabled.md) — 启用/停用智能助手
- [assistant-delete](assistant-delete.md) — 删除智能助手
