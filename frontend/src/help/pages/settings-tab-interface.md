# 界面显示 Tab

## 功能位置
更多设置页 →「界面显示」Tab（`InterfaceDisplayPanel`）

## 数据流图（前端 → 后端）

```mermaid
flowchart LR
  User["切换开关"] --> useConsolePanel["useConsolePanel"]
  useConsolePanel --> setVisible["setVisible(visible)"]
  setVisible --> LocalState["前端本地 state<br>底部执行日志面板显隐"]
```

## 调用关系链路图

```mermaid
flowchart TD
  SettingsPage["SettingsPage.tsx<br>SettingsPage()"] --> InterfaceTab["Tab interface"]
  InterfaceTab --> InterfaceDisplayPanel["InterfaceDisplayPanel.tsx"]
  InterfaceDisplayPanel --> useConsolePanel["useConsolePanel()<br>visible + setVisible"]
  useConsolePanel --> Switch["antd Switch<br>checked=visible<br>onChange=setVisible"]
```

## 数据结构图

```mermaid
classDiagram
  class ConsolePanelState {
    visible: boolean
  }
  note for ConsolePanelState "useConsolePanel hook 管理<br>底部执行日志面板显隐"
```

## 数据变更图

```mermaid
stateDiagram-v2
  [*] --> Shown: visible=true
  [*] --> Hidden: visible=false
  Shown --> Hidden: 切换开关
  Hidden --> Shown: 切换开关
```

## 开发指导
- **前端入口**：`frontend/src/components/settings/InterfaceDisplayPanel.tsx` 的 `InterfaceDisplayPanel` 组件；`useConsolePanel` hook 来自 `frontend/src/hooks/useConsolePanel`
- **后端入口**：无后端调用，纯前端 UI 偏好
- **注意**：关闭后即使有运行中任务也不会弹出底部日志框，日志数据仍在 `state.runningTasks` 中正常累积，重新打开即可立刻看到
- **扩展**：后续如有其他纯前端 UI 偏好（如列表密度、字号）可在本 Tab 追加对应开关
