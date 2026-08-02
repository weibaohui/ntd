# 执行器页子页签切换

## 功能位置
执行器页 → 顶部 `Tabs`（执行器 / API Key / 正在运行 / 会话）

## 数据流图（前端 → 后端）

```mermaid
flowchart LR
  User["点击 Tab"] --> setRunningTab["setRunningTab(key)"]
  setRunningTab --> Tab1["key=executors<br>配置表+运行配置+AI统计"]
  setRunningTab --> Tab2["key=api-key<br>ProfilesPanel"]
  setRunningTab --> Tab3["key=running<br>运行中任务表"]
  setRunningTab --> Tab4["key=sessions<br>SessionManager embedded"]
  Tab3 -->|"useEffect runningTab=running"| loadRunningRecords["loadRunningRecords()<br>10s 定时"]
```

## 调用关系链路图

```mermaid
flowchart TD
  ExecutorsPanel["ExecutorsPanel.tsx<br>ExecutorsPanel()"] --> runningTabState["useState runningTab<br>'executors'|'api-key'|'running'|'sessions'"]
  runningTabState --> Tabs["antd Tabs activeKey=runningTab"]
  Tabs --> ExecutorsTab["Tab key=executors<br>配置表+运行配置 Card+AI 使用统计 Card"]
  Tabs --> ApiKeyTab["Tab key=api-key<br>ProfilesPanel"]
  Tabs --> RunningTab["Tab key=running<br>运行中任务表"]
  Tabs --> SessionsTab["Tab key=sessions<br>SessionManager embedded"]
  RunningTab --> useEffectRunning["useEffect<br>runningTab=running 时启动 10s 定时"]
  useEffectRunning --> loadRunningRecords["loadRunningRecords()"]
```

## 数据结构图

```mermaid
classDiagram
  class ExecutorsPanel {
    +runningTab: string
    +setRunningTab(key)
  }
  note for ExecutorsPanel "runningTab = 'executors' | 'api-key' | 'running' | 'sessions'"
```

## 数据变更图

```mermaid
stateDiagram-v2
  [*] --> executors: 默认 useState
  executors --> api-key: 点击 API Key
  api-key --> executors: 点击执行器
  executors --> running: 点击正在运行
  running --> executors: 点击执行器
  executors --> sessions: 点击会话
  sessions --> executors: 点击执行器
```

## 开发指导
- **前端入口**：`frontend/src/components/settings/ExecutorsPanel.tsx` 的 `ExecutorsPanel` 组件，`Tabs onChange` 回调 `setRunningTab`
- **后端入口**：各 Tab 按需调用不同后端接口；Tab 切换本身无后端调用
- **注意**：`running` Tab 的 `useEffect` 仅在 `runningTab === 'running'` 时启动 10 秒定时器加载运行记录，切走时 clearInterval
- **扩展**：新增 Tab 时在 `Tabs items` 数组追加条目并扩展 `runningTab` 的 union 类型
