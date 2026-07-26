# 任务页重新设计

| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| AtomCode | 2026-07-26 | 初始版本：三态视图（列表 / 看板 / 卡片）重构方案 |

---

## 1. 背景

当前 `TasksPage` / `TaskDetailPage` 与整个 app 的设计语言脱节：

- 用 AntD 原生 `Card / List / Tabs / Typography.Title`，未使用项目统一的 `PageCard` / `ListDetailPage` 外壳
- `maxWidth: 800, margin: '0 auto'` 居中卡片布局，与「事项 / 环路 / 工艺」的全宽双栏布局不一致
- 无 `cursor: pointer`、无 hover transition、空态用 `List` 默认文案
- `TaskDetailPage` 用内部 `selectedTaskId` state 切换列表/详情，与项目其他页面的双栏联动模式不一致

**目标**：用与「事项中心」「看板」「工艺」一致的视觉与交互语言重新设计任务页。

---

## 2. 需求确认（用户决策）

| # | 问题 | 用户选择 |
|---|------|----------|
| 1 | 视图形态 | **三态视图**：列表（Table）/ 看板（按状态分泳道，仿 `MemorialBoard` 看板）/ 卡片（卡片墙，仿 `TodoCenterCardView`） |
| 2 | 新建入口 | 顶栏右上角 `+ 新建` 按钮 |
| 3 | 再次执行 | 保留现有 `createTaskExecution` Modal |
| 4 | 移动端 | 暂不加（保持桌面端 only，YAGNI） |

**关键纠正**：
- ❌ 不做「顶栏 Tabs 共用状态筛选」
- ✅ Table 自带 toolbar 状态筛选；卡片墙自带状态筛选；看板用状态做泳道（无需状态过滤）

---

## 3. 设计方案

### 3.1 整体架构

```
┌─ PageCard ─────────────────────────────────────────────┐
│ 🚀 任务               [搜索] [刷新] [Segmented] [+新建] │
│ ─────────────────────────────────────────────────────  │
│                                                         │
│  根据 viewMode 渲染：                                   │
│   • list   → TasksTableView  (Table + toolbar 筛选)    │
│   • kanban → TasksKanbanView (按状态分泳道)            │
│   • card   → TasksCardView   (卡片墙 + 状态筛选)       │
│                                                         │
│  详情:                                                  │
│   • list   → 选中行后右侧抽屉/下方展开 TaskDetailPanel │
│   • kanban → 卡片内联展开执行历史                      │
│   • card   → 点击卡片选中并切到 list 视图（参考 ItemsPage） │
└─────────────────────────────────────────────────────────┘
```

### 3.2 顶栏（PageCard extra）

```tsx
<>
  <Input size="small" prefix={<SearchOutlined />} allowClear
    placeholder="搜索任务标题或需求" style={{ width: 220 }}
    value={searchKeyword} onChange={e => setSearchKeyword(e.target.value)} />
  <Button size="small" icon={<ReloadOutlined />} onClick={reload}>刷新</Button>
  <Segmented size="small" value={viewMode} onChange={setViewMode}
    options={[
      { value: 'list',   icon: <UnorderedListOutlined />, title: '列表' },
      { value: 'kanban', icon: <AppstoreOutlined />,      title: '看板' },
      { value: 'card',   icon: <LayoutOutlined />,        title: '卡片' },
    ]} />
  <Button size="small" type="primary" icon={<PlusOutlined />} onClick={openCreateModal}>
    新建
  </Button>
</>
```

**视图模式持久化**：`localStorage['ntd_tasks_view']`，默认 `list`。

### 3.3 视图 1：列表态 `TasksTableView`

**形态**：AntD `Table` 单表格，桌面端主推。

| 列 | 字段 | 备注 |
|---|---|---|
| ID | `id` | 宽 60，`#1` 形式 |
| 标题 | `title` | `Text strong`，省略号 |
| 状态 | `status` | `<Tag color={statusColor}>` |
| 复杂度 | `complexity` | `<Tag>` light→green / standard→blue / complex→purple |
| 模板 | `template_name` | `<Tag>` 或 `—` |
| 最近执行 | `latest_execution_status` | `<Tag>` 同状态色板 |
| 创建时间 | `created_at` | `YYYY-MM-DD` |

**Table toolbar（自带筛选）**：
- 状态 `Select`：全部 / 待执行 / 进行中 / 已完成 / 失败
- 搜索框（也可走顶栏统一搜索，二选一）

**行交互**：
- `cursor: pointer`
- `onRow={{ onClick: () => onSelectTask(record.id) }}`
- 选中行高亮 `background: var(--color-primary-bg)`

**详情联动**：
- 选中任务后右栏渲染 `TaskDetailPanel`
- 用 `ListDetailPage` 双栏外壳（与「事项 / 环路」页一致）

### 3.4 视图 2：看板态 `TasksKanbanView`

**形态**：按 `status` 分泳道的横向看板，仿 `MemorialBoard` 的 `KanbanBoard`。

**泳道列**：
- `pending` 待执行
- `running` 进行中
- `success` 已完成
- `failed` 失败

**卡片内容**：
```
┌─────────────────────────┐
│ #12  [complexity]       │
│ 任务标题                │
│ [最新执行 Tag]          │
│ 创建时间                │
└─────────────────────────┘
```

- `cursor: pointer`
- hover `box-shadow` 增强 + `transform: translateY(-1px)` （`prefers-reduced-motion` 关闭）
- 点击卡片：选中该任务并在看板内 inline 展开「执行历史」（不跳转，参考 `KanbanBoard` 卡片展开）

**不做**：拖拽切状态（后端 `PATCH /tasks/:id` 未支持 status 更新，YAGNI）。

### 3.5 视图 3：卡片态 `TasksCardView`

**形态**：卡片墙网格，仿 `TodoCenterCardView`。

```css
.ntd-tasks-card-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(320px, 1fr));
  gap: 16px;
  padding: 16px;
}
```

**卡片内容**：
- 头部：`#id` + 状态 Tag
- 标题：`Title level={5}`
- 需求摘要：`Paragraph ellipsis={{ rows: 2 }}`
- 复杂度 Tag + 模板 Tag
- 底部：创建时间 + 「再次执行」按钮

**卡片墙筛选（自带）**：
- 状态 `Select`
- 关键词搜索（走顶栏统一 `searchKeyword`）

**点击卡片**：选中并切到 `list` 视图（参考 `ItemsPage` 卡片→列表行为）。

### 3.6 任务详情面板 `TaskDetailPanel`

复用现有 `TaskDetailPage` 内部内容，但改为「无返回按钮的嵌入面板」：

```
┌─ Descriptions 概览 ─────────────────────┐
│  模板 | 版本 | 复杂度 | 状态 | 创建时间  │
└──────────────────────────────────────────┘
┌─ 操作栏 ────────────────────────────────┐
│  [⚡ 再次执行]                            │
└──────────────────────────────────────────┘
┌─ Collapse 工艺要求（N 步） ──────────────┐
│  ▸ 步骤 1: 名称                          │
│     技能: [skill Tag...]                 │
│     产物: [artifact Tag...]              │
│     门禁: [gate Tag...]                  │
└──────────────────────────────────────────┘
┌─ 执行历史 ───────────────────────────────┐
│  • #1 success 8/8 完成  2026-07-25      │
│  • #2 running 4/8       2026-07-26      │
└──────────────────────────────────────────┘
  点查看详情 → 嵌入 <ProcessExecutionBoard />
```

### 3.7 状态色板（统一）

```ts
const STATUS_COLOR: Record<string, string> = {
  pending: 'default',
  running: 'blue',
  success: 'green',
  failed: 'red',
};
const STATUS_LABEL: Record<string, string> = {
  pending: '待执行',
  running: '进行中',
  success: '已完成',
  failed: '失败',
};
```

与 `ProcessExecutionBoard` 的色板保持一致。

### 3.8 新建任务 Modal

```
┌─ 新建任务 ───────────────────────────────┐
│  需求描述:                                │
│  ┌────────────────────────────────────┐  │
│  │ TextArea                             │  │
│  └────────────────────────────────────┘  │
│  工艺环路: [Select ▾]                     │
│                                           │
│              [取消]  [创建任务]           │
└───────────────────────────────────────────┘
```

- 复用 `bundledApi.createTask(requirement, loopId, wsId)` API
- 创建成功 `message.success` 后刷新列表
- 复杂度由后端模板自动判定，前端不传

---

## 4. 文件改动清单

### 4.1 新增文件

| 文件 | 职责 |
|------|------|
| `frontend/src/components/tasks/TasksTableView.tsx` | 列表态：Table + 状态筛选 |
| `frontend/src/components/tasks/TasksKanbanView.tsx` | 看板态：按状态分泳道 |
| `frontend/src/components/tasks/TasksCardView.tsx` | 卡片态：卡片墙 |
| `frontend/src/components/tasks/TaskDetailPanel.tsx` | 嵌入式详情面板（无返回按钮） |
| `frontend/src/components/tasks/CreateTaskModal.tsx` | 新建任务 Modal |
| `frontend/src/components/tasks/constants.tsx` | 状态色板、复杂度色板、共享类型 |
| `frontend/src/components/tasks/__tests__/constants.test.ts` | 单元测试 |

### 4.2 修改文件

| 文件 | 改动 |
|------|------|
| `frontend/src/components/tasks/TasksPage.tsx` | 重写：PageCard 外壳 + 顶栏 extra + 三态视图切换 + ListDetailPage 双栏（列表态） |
| `frontend/src/components/tasks/TaskDetailPage.tsx` | 重写：改为 `TaskDetailPanel`，去掉外层 padding/返回按钮 |
| `frontend/src/App.tsx` | 微调 props 透传（如有需要） |

### 4.3 删除文件

无（保留 `TaskDetailPage.tsx` 重命名为 `TaskDetailPanel.tsx`，内容重构）。

---

## 5. 函数拆分（每函数 ≤ 30 行）

### 5.1 `TasksPage.tsx`

```ts
export function TasksPage({ workspaceId }: TasksPageProps) {
  // 1. 状态：viewMode, searchKeyword, refreshKey, selectedTaskId
  // 2. 派发：reload signal

  const extra = renderHeaderExtra({ searchKeyword, ... });
  const listPanel = <TasksTableView ... />;
  const detailPanel = selectedTaskId ? <TaskDetailPanel ... /> : <EmptyDetailPlaceholder />;

  if (viewMode === 'kanban') return <PageCard ...><TasksKanbanView ... /></PageCard>;
  if (viewMode === 'card')   return <PageCard ...><TasksCardView   ... /></PageCard>;

  // list 视图：双栏
  return <ListDetailPage icon title="任务" listPanel={listPanel} detailPanel={detailPanel} extra={extra} />;
}
```

### 5.2 `TasksTableView.tsx`

```ts
export function TasksTableView({ ... }) {
  const { tasks, loading, statusFilter, setStatusFilter } = useTasksTableState(workspaceId, searchKeyword);
  const columns = buildTaskColumns({ onSelectTask });
  return (
    <div style={{ padding: 16 }}>
      <TableToolbar ... />
      <Table rowKey="id" columns={columns} dataSource={tasks}
        loading={loading} onRow={...} />
    </div>
  );
}
```

### 5.3 `TasksKanbanView.tsx`

```ts
export function TasksKanbanView({ ... }) {
  const { tasks, loading, selectedTaskId } = useTasksKanbanState(workspaceId);
  const lanes = buildKanbanLanes(tasks);  // pending/running/success/failed
  return (
    <div className="ntd-tasks-kanban">
      {lanes.map(lane => (
        <KanbanLane key={lane.status} title={lane.title} count={lane.items.length}>
          {lane.items.map(task => <KanbanTaskCard key={task.id} task={task} ... />)}
        </KanbanLane>
      ))}
    </div>
  );
}
```

### 5.4 `TasksCardView.tsx`

```ts
export function TasksCardView({ ... }) {
  const { tasks, loading, statusFilter, setStatusFilter } = useTasksCardState(workspaceId, searchKeyword);
  return (
    <div className="ntd-tasks-card-container">
      <CardToolbar ... />
      <div className="ntd-tasks-card-grid">
        {tasks.map(task => <TaskCard key={task.id} task={task} onSelect={...} />)}
      </div>
    </div>
  );
}
```

### 5.5 `TaskDetailPanel.tsx`

```ts
export function TaskDetailPanel({ taskId, workspaceId, onTriggered }: TaskDetailPanelProps) {
  const { detail, loading } = useTaskDetail(workspaceId, taskId);
  if (loading) return <Spin />;
  if (!detail) return <Empty />;
  return (
    <div style={{ padding: 24, height: '100%', overflow: 'auto' }}>
      {renderSummaryCard(detail)}
      {renderActionButtons(detail, onTriggered)}
      {renderStepsCollapse(detail.steps)}
      {renderExecutionHistory(detail.executions)}
    </div>
  );
}
```

---

## 6. UI/UX Pro Max 检查清单

### 6.1 视觉质量
- [x] 不用 emoji 作图标（用 `@ant-design/icons`）
- [x] 图标统一来自 AntD Icon set
- [x] hover 状态不引起 layout shift（只用 color / box-shadow / transform）
- [x] 主题色直接用 `var(--color-primary)` 等 CSS 变量

### 6.2 交互
- [x] 所有可点击卡片/行都有 `cursor: pointer`
- [x] hover 有清晰视觉反馈
- [x] transition 150-300ms

### 6.3 明暗模式
- [x] 文本对比度 ≥ 4.5:1
- [x] 玻璃/透明元素在 light mode 可见
- [x] border 在两种模式下都可见

### 6.4 布局
- [x] 浮动元素有合适的边距
- [x] 内容不被 fixed navbar 遮挡
- [x] 响应式 375px / 768px / 1024px / 1440px

### 6.5 可访问性
- [x] 所有图片有 alt text
- [x] form inputs 有 labels
- [x] 颜色不是唯一指示（同时有文字 / 图标）
- [x] `prefers-reduced-motion` 受尊重

---

## 7. 安全反思

- ✅ 不引入新的 API 调用，复用现有 `bundledApi.createTask` / `listTasks` / `getTaskDetail` / `createTaskExecution`
- ✅ 无 SQL 注入风险（前端不直接构造 SQL）
- ✅ XSS：任务标题/需求来自后端，React 默认转义；不使用 `dangerouslySetInnerHTML`
- ✅ 权限：所有 API 走后端 workspace 校验，前端不做额外鉴权
- ✅ 输入校验：新建任务 Modal 在前端校验非空，后端再做一次校验

---

## 8. 已知限制 / 待改进

1. **看板不支持拖拽**：后端 `PATCH /tasks/:id` 未实现 status 更新。后续若加拖拽，需后端补 API。
2. **卡片墙状态筛选**：当前只过滤 status，未做复杂度/模板筛选。后续按需扩展。
3. **任务详情移动端**：当前未做移动端适配。后续若启用，需补 `TasksMobilePage`。
4. **任务搜索**：当前只在前端 filter。若任务量大，需后端补 `?search=` 参数。
