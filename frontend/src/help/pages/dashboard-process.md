# 工艺 Tab

## 功能位置

仪表盘 → 「工艺」Tab → `ProcessDashboard` 组件（模板总数 / 总安装次数 / 最受欢迎 / 模板使用排行表）

## 数据流图（前端 → 后端）

```mermaid
flowchart LR
  U[用户切工艺 Tab] --> PD["ProcessDashboard"]
  PD --"useEffect load"--> LS["setLoading(true)"]
  LS --"getProcessStats()"--> API["/api/v1/processes/stats"]
  API --> H["get_process_stats handler"]
  H --"db list_process_templates + loop_count 聚合"--> DB1[(process_templates 表)]
  H --"count loops by template"--> DB2[(loops 表)]
  H --> RT["返回 template_stats / total_templates"]
  RT --> PD["setStats(data)"]
  PD --> CR["卡片 3 个 总数/总安装/最受欢迎"]
  PD --> TB["Table 模板使用排行"]
```

## 调用关系链路图

```mermaid
flowchart TD
  PD["ProcessDashboard useEffect([])"] --> LOAD["load async"]
  LOAD --> API["bundledApi.getProcessStats()"]
  API --> GET["GET /api/v1/processes/stats"]
  GET --> H["backend get_process_stats"]
  H --> AGG["聚合 template_stats + total_templates"]
  LOAD --> SET["setStats(data)"]
  SET --> CR["Row 3 卡片 Col span=8"]
  SET --> TB["Table columns: display_name/complexity/loop_count"]
  TB --> EXP["expandable 进度条 maxCount 基准"]
```

## 数据结构图

```mermaid
classDiagram
  class ProcessStats {
    +template_stats: TemplateStat[]
    +total_templates: number
  }
  class TemplateStat {
    +name: string
    +display_name: string
    +complexity: string
    +loop_count: number
  }
  class ProcessDashboard {
    +stats: ProcessStats | null
    +loading: boolean
  }
  ProcessDashboard --> ProcessStats: getProcessStats 加载
  ProcessStats --> TemplateStat: 列表项
```

## 数据变更图

```mermaid
stateDiagram-v2
  [*] --> 加载中
  加载中 --> 已渲染: getProcessStats 成功
  加载中 --> 已渲染: 失败 stats=null 静默
  已渲染 --> 已渲染: 切 Tab 不重拉 useEffect 空依赖
  已渲染 --> [*]
```

## 开发指导

- **前端入口**：`frontend/src/components/process/ProcessDashboard.tsx` 的 `ProcessDashboard` 组件；`bundledApi.getProcessStats` 在 `frontend/src/api/bundled.ts`
- **后端入口**：`backend/src/handlers/process.rs` 的 `get_process_stats` handler，路由 `GET /api/v1/processes/stats`
- **注意**：统计接口可选，失败静默（`catch` 空体）不弹错；进度条用 `maxCount` 基准防空数据除零；`useEffect` 空依赖数组只在挂载拉一次，切 Tab 不重拉
- **扩展**：增统计维度时，改后端 `get_process_stats` handler 聚合新字段、`ProcessStats` / `TemplateStat` 加列、前端 Table columns 加项
