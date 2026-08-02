# 绑定智能助手

## 功能位置
智能助手页 → 页面右上角「绑定智能助手」按钮（`PlusOutlined`，type=primary）

## 数据流图（前端 → 后端）

```mermaid
flowchart LR
  User["点击绑定"] --> handleStartBind["handleStartBind"]
  handleStartBind -->|"db.feishuInit"| API1["POST /api/v1/agent-bots/feishu/init"]
  API1 -->|"supported?"| Check{supported?}
  Check -->|"false"| ErrorMsg["pollError 显示不支持"]
  Check -->|"true"| Begin["db.feishuBegin"]
  Begin -->|"POST"| API2["POST /api/v1/agent-bots/feishu/begin"]
  API2 --> QRCode["QRCode.toDataURL<br>qr_url"]
  QRCode --> SSE["db.feishuPollSSE<br>EventSource"]
  SSE -->|"GET"| API3["GET /api/v1/agent-bots/feishu/poll"]
  API3 -->|"pollRes.success"| loadData["loadData() 刷新"]
  API3 -->|"pollRes.error"| PollError["pollError 显示"]
```

## 谑用关系链路图

```mermaid
flowchart TD
  Page["AssistantManagementPage.tsx<br>AssistantManagementPage()"] --> handleStartBind["handleStartBind"]
  handleStartBind --> cleanup["clearTimeout + close EventSource"]
  handleStartBind --> setBinding["setBinding(true)<br>setBindModalOpen(true)"]
  handleStartBind --> feishuInit["db.feishuInit()"]
  feishuInit --> supportedCheck{supported?}
  supportedCheck -->|"false"| setError1["setPollError<br>不支持 client_secret"]
  supportedCheck -->|"true"| feishuBegin["db.feishuBegin()"]
  feishuBegin --> qrCode["QRCode.toDataURL<br>beginRes.qr_url"]
  qrCode --> setQrCodeUrl["setQrCodeUrl"]
  qrCode --> feishuPollSSE["db.feishuPollSSE<br>device_code/interval/expire_in"]
  feishuPollSSE --> successCb["pollRes.success → loadData<br>2s 后关弹窗"]
  feishuPollSSE --> errorCb["pollRes.error → setPollError"]
  feishuPollSSE --> sseErrorCb["error → setPollError"]
```

## 数据结构图

```mermaid
classDiagram
  class FeishuBeginResponse {
    qr_url: string
    device_code: string
    interval: number
    expire_in: number
  }
  class FeishuPollResult {
    success: boolean
    bot_name: string
    error: string
  }
```

## 数据变更图

```mermaid
stateDiagram-v2
  [*] --> Idle: 页面加载
  Idle --> Binding: 点击绑定
  Binding --> ErrorNotSupported: feishuInit supported=false
  Binding --> QRReady: feishuBegin 成功生成二维码
  QRReady --> SSEPolling: 启动 EventSource
  SSEPolling --> BindSuccess: pollRes.success
  BindSuccess --> Idle: 2s 后关弹窗 loadData
  SSEPolling --> ErrorState: pollRes.error 或 SSE 失败
  ErrorState --> Idle: 用户关闭弹窗
```

## 开发指导
- **前端入口**：`frontend/src/components/assistant-management/AssistantManagementPage.tsx` 的 `handleStartBind` 回调；`feishuPollSSE` 来自 `frontend/src/utils/database/bots.ts`
- **后端入口**：`backend/src/handlers/agent_bot.rs` 处理 `feishu/init`、`feishu/begin`、`feishu/poll`（SSE）
- **注意**：绑定时不传 `workspaceId`，绑定的 Bot 默认不分配工作空间，用户可在配置抽屉中选择服务工作空间；组件卸载时必须 `feishuEventSource?.close()` 和 `clearTimeout` 清理资源
- **扩展**：新增其他平台绑定时复用二维码 + SSE 轮询模式，`bot_type` 追加值并在 `feishuInit` 之前做平台分发
