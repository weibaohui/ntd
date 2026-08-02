# Tab 切换（7 个语义域）

## 功能位置

仪表盘 → `Tabs` 控件（card 型，7 个 Tab：总览 / 任务 / 执行 / 成本与模型 / 自动化 / 资源与运维 / 工艺）

## 数据流图（前端 → 后端）

```mermaid
flowchart LR
  U[用户点 Tab] --> HTC["handleTabChange(key)"]
  HTC --"pushUrl('dashboard', { tab: key })"--> URL["URL hash 写 tab 参数"]
  URL --> UV["useViewState activeTab"]
  UV --> RT["resolvedTab 校验 DASHBOARD_TABS"]
  RT --> TIT["tabItems 渲染对应 Tab 组件"]
  TIT --> OC["OverviewTab stats/successRate/runningTasks/todos"]
  TIT --> TK["TasksTab stats/totalTodos"]
  TIT --> EX["ExecutionsTab stats/tagsLength"]
  TIT --> CO["CostTab stats + UsageStatsCard since/until"]
  TIT --> AU["AutomationTab msgStats/hours"]
  TIT --> RE["ResourcesTab stats"]
  TIT --> PR["ProcessDashboard"]
```

## 调用关系链路图

```mermaid
flowchart TD
  T["Tabs onChange"] --> HTC["handleTabChange useCallback"]
  HTC --> PU["pushUrl('dashboard', { tab: key })"]
  PU --> UV["useViewState 解析 activeTab"]
  UV --> RT["resolvedTab: DASHBOARD_TABS.includes 校验 非法回退 overview"]
  RT --> AK["Tabs activeKey=resolvedTab"]
  AK --> TI["tabItems 数组 按序渲染"]
  TI --> RL["renderLabel(Icon, full, short) 移动端短文案"]
```

## 数据结构图

```mermaid
classDiagram
  class DASHBOARD_TABS {
    +overview: 总览
    +tasks: 任务
    +executions: 执行
    +cost: 成本与模型
    +automation: 自动化
    +resources: 资源与运维
    +process: 工艺
  }
  class DashboardTabKey {
    +key: DASHBOARD_TABS[number]
  }
  class TabItem {
    +key: DashboardTabKey
    +label: JSX
    +children: JSX
  }
  DASHBOARD_TABS --> DashboardTabKey: as const 联合类型
  DashboardTabKey --> TabItem: 每个 key 对应一项
```

## 数据变更图

```mermaid
stateDiagram-v2
  [*] --> overview: 默认 / URL 非法值回退
  overview --> tasks: 点 Tab
  tasks --> executions: 点 Tab
  executions --> cost: 点 Tab
  cost --> automation: 点 Tab
  automation --> resources: 点 Tab
  resources --> process: 点 Tab
  process --> overview: 点 Tab
  [*] --> tasks: URL hash 携合法 tab
```

## 开发指导

- **前端入口**：`frontend/src/components/Dashboard.tsx` 的 `Dashboard` 组件；`DASHBOARD_TABS` 常量、`tabItems` 数组、`handleTabChange` / `resolvedTab` 校验
- **后端入口**：Tab 切换纯前端，不直接调后端；各 Tab 内部数据拉取见对应 Tab 子文档
- **注意**：Tab 切换写入 URL hash（`pushUrl`），浏览器前进/后退/刷新保持当前 Tab；`resolvedTab` 校验非法/缺失值回退 `overview`，保证不渲染空白；移动端用短文案（如「成本与模型」→「成本」）避免 6 个 Tab 在窄屏溢出
- **扩展**：新增 Tab 时，扩 `DASHBOARD_TABS` as const 数组、`tabItems` 加项、对应 Tab 组件文件、子文档 md
