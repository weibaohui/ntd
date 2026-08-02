# 时间窗过滤

## 功能位置

任务（列表） → 顶栏时间分段 `TimeRangeSegmented`（`showAll` 形态，首项「全部」+ 6h/12h/24h/3d/7d）

## 数据流图（前端 → 后端）

```mermaid
flowchart LR
  U([用户点时间分段选项]) -->|"onChange hours"| page["TasksPage"]
  page -->|"setHours(number|null)"| state["hours state<br/>null = 全部不过滤"]
  state -->|"useMemo timeFilteredTasks<br/>deps: tasks, hours"| filter["页级时间过滤"]
  filter -->|"hours === null"| passthrough["原样透传 tasks"]
  filter -->|"hours != null<br/>cutoff = Date.now() - hours*3600*1000"| date_filter["tasks.filter(t =><br/>ts = new Date(created_at).getTime()<br/>!isNaN(ts) && ts >= cutoff)"]
  passthrough -->|"三态视图共享"| views["TasksTableView<br/>TasksKanbanView<br/>TasksCardView"]
  date_filter --> views
  views -.->|"后端不参与<br/>过滤在前端做"| api["bundledApi.listTasks<br/>仅 reload 时全量拉取"]
```

## 调用关系链路图

```mermaid
flowchart TD
  seg["TimeRangeSegmented<br/>showAll: true<br/>value: hours<br/>onChange: setHours"] -->|"内部 handleChange"| mapping["toSegmentedValue(props)<br/>number|null → Segmented 字符串值"]
  mapping -->|"用户点击"| change["handleChange(v)"]
  change -->|"v === '__all__'"| null_out["props.onChange(null)"]
  change -->|"v === '6h'/'12h'/...<br/>逆映射 TIME_RANGE_OPTIONS"| num_out["props.onChange(opt.value)"]
  null_out --> set_hours["TasksPage setHours(null)"]
  num_out --> set_hours2["TasksPage setHours(number)"]
  set_hours --> memo["useMemo timeFilteredTasks<br/>deps: tasks, hours"]
  set_hours2 --> memo
  memo -->|"hours == null"| pass["return tasks"]
  memo -->|"hours != null"| cutoff["cutoff = Date.now() - hours * 3600 * 1000<br/>tasks.filter(t => {<br/>  ts = t.created_at ? new Date(t.created_at).getTime() : NaN<br/>  return !Number.isNaN(ts) && ts >= cutoff<br/>})"]
  pass --> views["三态视图<br/>timeFilteredTasks prop"]
  cutoff --> views
```

## 数据结构图

```mermaid
classDiagram
  class TIME_RANGE_OPTIONS {
    <<constant>>
    +6h: 6
    +12h: 12
    +24h: 24
    +3d: 72
    +7d: 168
  }
  class TimeRangeSegmented_showAll {
    +showAll: true
    +value: number|null
    +onChange: (hours: number|null) => void
  }
  class TasksPage_HoursState {
    +hours: number|null
    +setHours: function
  }
  class TaskItem {
    +created_at?: string
  }
  TimeRangeSegmented_showAll --> TasksPage_HoursState : setHours
  TasksPage_HoursState --> TIME_RANGE_OPTIONS : 值域来源
  TasksPage_HoursState --> TaskItem : 按 created_at 过滤
  note for TimeRangeSegmented_showAll "null = 全部不过滤\n哨兵串 '__all__' 映射 null"
```

## 数据变更图

```mermaid
stateDiagram-v2
  [*] --> 全部: hours = null（默认）
  全部 --> 6h: 点 6h
  全部 --> 12h: 点 12h
  全部 --> 24h: 点 24h
  全部 --> 3d: 点 3d
  全部 --> 7d: 点 7d
  6h --> 全部: 点「全部」
  12h --> 全部: 点「全部」
  24h --> 6h: 点 6h
  7d --> 全部: 点「全部」
  note right of 全部: 不持久化\n管理视角默认收窄会让老任务消失
  note right of 6h: cutoff = Date.now() - 6*3600*1000\ncreated_at >= cutoff 才保留\ncreated_at 缺失/非法视为不在窗口
```

## 开发指导

- **前端入口**：`frontend/src/components/tasks/TasksPage.tsx` 的 `timeRangeSegment` 常量（`TimeRangeSegmented` `showAll` 形态），`hours` state 默认 `null`（全部不过滤），页级过滤在 `timeFilteredTasks` `useMemo`；时间分段组件定义在 `frontend/src/components/common/TimeRangeSegmented.tsx`，选项集 `TIME_RANGE_OPTIONS` 全站唯一事实源
- **后端入口**：无后端调用。时间过滤完全在前端做，后端 `list_tasks` 不支持时间参数
- **注意**：`hours` 不持久化（与看板页 `hours` 不持久化现状一致，需求 031 结论 2C）；`created_at` 缺失或非法时 `new Date().getTime()` 返回 `NaN`，filter 中 `!Number.isNaN(ts)` 判定将其排除（与 KanbanBoard 对非法时间 NaN-drop 处理对齐）；过滤放在页级而非各视图内，与 `searchKeyword` 的共享方式一致，三态视图 props 零改动
- **扩展**：新增时间选项只改 `frontend/src/components/common/TimeRangeSegmented.tsx` 的 `TIME_RANGE_OPTIONS` 一处，所有使用页面（任务页/看板页）同步生效；若需后端时间过滤，在 `list_tasks` DAO 加 `created_at >= cutoff` 条件，前端 `reload` 透传 `hours`
