# 关系图

> **位置**：Todo 列表 → 顶部「关系图」切换
> **前端**：`frontend/src/components/relation-map/RelationMap.tsx`
> **后端图构建**：`frontend/src/components/relation-map/GraphBuilder.ts`

把 Todo 与各触发源之间的**关联**画成图谱。适合看一个大任务被拆成几个子任务的结构，以及 Webhook / 飞书 / 调度器对 Todo 的触发关系。

## 1. 节点

### 1.1 Todo 节点

- 每个节点 = 一个 Todo
- 颜色按状态（`Nodes.tsx:16-22`）：
  - `pending` 灰 `#8c8c8c`
  - `in_progress` 蓝 `#1677ff`
  - `running` 蓝 `#1677ff`
  - `completed` 绿 `#52c41a`
  - `failed` 红 `#ff4d4f`
- 节点大小固定，不随子任务数变化
- 节点底部用色块标注执行器（来自 `EXECUTOR_COLORS`）

### 1.2 触发源节点类型

| 类型 | 图标 | 颜色 | 含义 |
|------|------|------|------|
| `webhook` | ApiOutlined | `#722ed1` | Webhook 触发源（节点显示「已启用/已禁用」标签） |
| `feishu` | MessageOutlined | `#1890ff` | 飞书斜杠命令节点（附 `/command` 标签） |
| `feishu`（default_response） | MessageOutlined | `#1890ff` | 飞书默认响应节点 |
| `scheduler` | ScheduleOutlined | `#fa8c16` | 调度器节点（附 Cron 表达式） |

## 2. 边

- **Hook 边**：单向，触发源 Todo → 目标 Todo，边上标注触发条件：`completed` / `failed` / `pending` / `in_progress`（`GraphBuilder.ts:50-114`，`Nodes.tsx:120-145`）
- **Webhook 边**：从 Webhook 节点 → 默认 Todo
- **飞书边**：从飞书节点 → 目标 Todo
- **调度器边**：从调度器节点 → 自身 Todo

边类型通过颜色和线型区分（图例见页面左下角面板）：

- Hook：绿色实线
- Webhook：紫色虚线
- 飞书：蓝色点线
- 调度：橙色长虚线

## 3. 交互

- 拖拽节点改变位置
- 滚轮缩放（`minZoom=0.1, maxZoom=2`）
- 鼠标 hover 边 / 节点看关联类型
- 右上角过滤开关（独立 4 个 Switch）：
  - Hook
  - Webhook
  - 飞书
  - 调度

> 开关关闭时对应的节点和边都不会被构建出来（`RelationMap.tsx:170-193`）。

## 4. 布局

- 自定义**分层布局**（`GraphBuilder.ts:246-353`）
- 算法步骤：
  1. BFS 按入度分层：source 节点（无入边）放第 0 层
  2. 同一层内按节点类型排序：webhook → feishu → scheduler → todo
  3. 节点 `x = layer * 280`，`y = startY + i * 100`
  4. 含环路保护：`layer > nodes.length` 时停止提升

## 5. 与 Hook 系统的关系

Todo 的前后置 hook 也会显示在关系图里。详细看 [Hook 系统设计](../../../hook-system-design.md)（`features/relation-map.md` 回退 3 层到 `docs/`）。

## 6. 故障排查

### 6.1 节点全聚一起

- 关联太多太密
- 用右上角过滤开关关掉部分类型，缩小可视节点数

### 6.2 边不显示

- 检查对应过滤开关是否打开
- 看 Todo 详情 → Hook 编辑器是否有内容

### 6.3 卡顿

- Todo 太多，关掉部分过滤类型
- 关闭浏览器其他 tab

