# Dashboard 全局视图菜单位置调整设计

> 版本：v1
> 日期：2026-07-23
> 状态：实施中

---

## 1. 问题背景

Dashboard（仪表盘）在产品文档中被定义为「全局运营视图」，但当前前端菜单把它放在左侧导航的「工作区」section 内，与事项、环路、消息、黑板、看板并列。这带来两个语义冲突：

1. **UI 层级冲突**：用户切换工作区时，会误以为 Dashboard 数据会随当前 workspace 变化。
2. **数据范围冲突**：后端 `db/dashboard.rs` 的统计 SQL 目前是全库聚合，没有按 `workspace_id` 过滤；`execution.rs` handler 中也有 `TODO(ws-isolation)` 注释明确说明当前未实现 workspace 隔离。

因此 Dashboard 事实上是全局数据，但路由和菜单都伪装成 workspace-scoped，需要归正。

---

## 2. 设计目标

- 让导航层级与数据范围一致：Dashboard 作为全局视图，在菜单中有独立分组。
- 切换工作区不影响 Dashboard 数据。
- 后端路由与前端调用同步改为全局，消除「伪 workspace-scoped」状态。
- 保持现有工作区级视图（items / loops / messages / blackboard / memorial）不变。

---

## 3. 方案选型

最终选择 **方案 B：在 LeftRail 中新增「全局视图」section**。

### 3.1 为什么不选方案 A（顶部全局入口 / 主页）

- 需要调整 LeftRail 顶部结构或新增 App 顶部 header，改动面更大。
- 当前 Dashboard 已经是默认兜底视图（`/#/` 不匹配时渲染 Dashboard），「首页」语义已有部分覆盖，无需额外突出。

### 3.2 为什么不选方案 C（与 memorial 合并为「洞察」组）

- memorial（看板）从调用链看是 workspace-scoped（必须传 `workspaceId`）。
- 把全局 Dashboard 和工作区级 memorial 放在同一组，切换 workspace 时会出现同一分组内行为不一致，反而加深困惑。

### 3.3 方案 B 优势

- 语义清晰：「全局视图」与「工作区」「配置」并列。
- 改动最小：仅在 `LeftRail.tsx` 增加一个 section，路由、视图状态、Dashboard 组件复用。
- 可扩展：未来「全局日志」「全局搜索」等可直接归入该 section。

---

## 4. 改动范围

### 4.1 前端

| 文件 | 变更 |
|------|------|
| `frontend/src/components/shell/LeftRail.tsx` | 在「工作区」与「配置」之间新增「全局视图」section，仅含 Dashboard。 |
| `frontend/src/components/Dashboard.tsx` | 移除 `state.selectedWorkspace` 依赖，调用新的全局 stats 接口。 |
| `frontend/src/utils/database/executions.ts` | `getDashboardStats` 不再接收 `workspaceId`，请求 `/api/v1/stats/dashboard`。 |

### 4.2 后端

| 文件 | 变更 |
|------|------|
| `backend/src/handlers/action.rs` | 将 dashboard stats 路由从 `/api/v1/workspaces/{ws}/stats/dashboard` 改为 `/api/v1/stats/dashboard`。 |
| `backend/src/handlers/execution.rs` | `get_dashboard_stats` handler 移除 `ws_id` 路径参数和 workspace 缓存键。 |

### 4.3 文档

| 文件 | 变更 |
|------|------|
| `docs/user-guide/features/dashboard.md` | 更新 API 路径为 `/api/v1/stats/dashboard`，说明 Dashboard 为全局视图。 |

---

## 5. 新的菜单结构

```
┌────────────────────────────────────────────┐
│  工作区 ▼ 工作区 A                          │
├────────────────────────────────────────────┤
│  工作区                                      │
│  ├─ 事项                                     │
│  ├─ 环路                                     │
│  ├─ 消息                                     │
│  ├─ 黑板                                     │
│  └─ 看板                                     │
├────────────────────────────────────────────┤
│  全局视图                                    │
│  └─ 仪表盘  ← 从工作区抽出，数据全局聚合    │
├────────────────────────────────────────────┤
│  配置                                        │
│  ├─ 技能                                     │
│  └─ 专家                                     │
├────────────────────────────────────────────┤
│  ≡ 配置  [主题]  [收起]                      │
└────────────────────────────────────────────┘
```

---

## 6. 用户旅程

1. 用户登录 ntd，左侧导航显示独立的「全局视图」分组。
2. 用户点击「全局视图 → 仪表盘」。
3. 路由跳转到 `/#/dashboard`，调用 `/api/v1/stats/dashboard` 加载全局聚合数据。
4. 用户切换工作区：工作区 section 内视图随 workspace 变化，Dashboard 保持全局数据不变。

---

## 7. 兼容性说明

- 旧路由 `/api/v1/workspaces/{ws}/stats/dashboard` 不再保留（符合 ADR-7：旧路由不保留，一次性切换）。
- CLI `ntd stats --workspace-id` 暂不受影响：该命令调用的是 CLI 自己的路径构造，需要同步检查是否指向新全局路由。
