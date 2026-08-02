# 防抖双进度条

## 功能位置

黑板页 → 桌面端标题栏内 `BlackboardDebounceBar`（双 Progress 进度条）；移动端标题后缀 `MobileDebounceIndicator`（文字指示器）

## 数据流图（前端 → 后端）

```mermaid
flowchart LR
  WS["WebSocket 推送 BlackboardDebounceStatus"] --> EVT["useExecutionEvents 解析事件"]
  EVT --> DISP["window.dispatchEvent CustomEvent blackboardDebounceStatus"]
  DISP --> LIST["BlackboardDebounceBar / MobileDebounceIndicator 监听"]
  LIST --> FILTER["按 workspace_id 过滤事件"]
  FILTER --> SET["setStatus(s)"]
  SET --> RENDER["渲染时间进度条 + 条数进度条"]
```

## 调用关系链路图

```mermaid
flowchart TD
  WS["WebSocket message"] --> useExecutionEvents["useExecutionEvents case BlackboardDebounceStatus"]
  useExecutionEvents --> dispatch["window.dispatchEvent CustomEvent"]
  dispatch --> BlackboardDebounceBar["addEventListener blackboardDebounceStatus"]
  dispatch --> MobileDebounceIndicator["addEventListener blackboardDebounceStatus"]
  BlackboardDebounceBar --> workspace_check["s.workspace_id !== workspaceId → return"]
  workspace_check --> setStatus["setStatus(s)"]
  setStatus --> timeProgress["时间进度条 Progress"]
  setStatus --> countProgress["条数进度条 Progress"]
  setStatus --> detailPopup["showDetail 气泡详情"]
  MobileDebounceIndicator --> workspace_check2["workspace_id 过滤"]
  workspace_check2 --> setStatus2["setStatus(s)"]
  setStatus2 --> textRender["文字渲染 刷新中/待刷/倒计时"]
```

## 数据结构图

```mermaid
classDiagram
  class BlackboardDebounceStatus {
    +workspace_id: number
    +pending_count: number
    +threshold: number
    +debounce_secs: number
    +remaining_secs: number
    +refreshing: boolean
  }
  class BlackboardDebounceBarProps {
    +workspaceId: number
  }
  class MobileDebounceIndicatorProps {
    +workspaceId: number
  }
  BlackboardDebounceBar --> BlackboardDebounceStatus
  MobileDebounceIndicator --> BlackboardDebounceStatus
```

## 数据变更图

```mermaid
stateDiagram-v2
  [*] --> NoStatus: status = null → 不渲染
  NoStatus --> Waiting: 收到事件 pending_count > 0 remaining_secs >= 0
  Waiting --> ThresholdMet: pending_count >= threshold
  Waiting --> Refreshing: refreshing = true
  ThresholdMet --> Refreshing: 后端触发 flush
  Refreshing --> Waiting: flush 完成 pending_count 重置
  Waiting --> NoStatus: pending_count = 0 remaining_secs < 0
  ThresholdMet --> NoStatus: flush 完成清空
```

## 开发指导

- **前端入口**：`frontend/src/components/BlackboardPage.tsx` 的 `BlackboardDebounceBar`（桌面端）和 `MobileDebounceIndicator`（移动端）组件；事件源自 `frontend/src/hooks/useExecutionEvents.ts` 的 `BlackboardDebounceStatus` case
- **后端入口**：后端 WebSocket 在防抖周期内推送 `BlackboardDebounceStatus` 事件，含 `pending_count` / `threshold` / `debounce_secs` / `remaining_secs` / `refreshing` 字段
- **注意**：事件用 `window.addEventListener('blackboardDebounceStatus', handler)` 监听，组件卸载时必须 `removeEventListener` 清理；`remaining_secs = -1` 表示无 active timer，此时时间进度条不展示数值；`refreshing = true` 时进度条变绿色并 `status='active'` 动画；移动端用文字替代进度条以节省空间
- **扩展**：若需在进度条旁展示「立即触发」按钮，在 `BlackboardDebounceBar` 中追加按钮调用后端 flush 接口；新增防抖状态字段时在 `BlackboardDebounceStatus` 接口和后端事件推送中同步添加
