# 跳转环路详情

## 功能位置

环路（列表） → `LoopListView` Table 行点击（`onRow` → `onSelectLoop(record.id)`）或「名称」列单元格内的 `<a>` 链接（`onClick` 内 `e.stopPropagation()` 后调 `onSelectLoop(record.id)`）。

## 数据流图（前端 → 后端）

```mermaid
flowchart LR
  U[用户点击行或名称链接] --> OR["buildOnRow onRow.onClick"]
  U --> NM["名称列 a onClick stopPropagation"]
  OR --> SL["onSelectLoop record.id"]
  NM --> SL
  SL --> HSL["App handleSelectLoop"]
  HSL --> CS["clearSelection"]
  CS --> PU["pushUrl loops id"]
  PU --> URL["hash 路由 #/loops/{id}"]
  URL --> APP["App useViewState 解析 loopDetailId"]
  APP --> LDP["渲染 LoopDetailPage"]
  LDP --> DBL["dbLoops.getLoop 拉详情"]
```

## 调用关系链路图

```mermaid
flowchart TD
  LV["LoopListView onRow buildOnRow + 名称列 a"] -->|"onSelectLoop(id)"| LLP["LoopListPage onSelectLoop"]
  LLP -->|"App 注入"| APP["App.tsx handleSelectLoop"]
  APP -->|"clearSelection"| CS["dispatch 清选中态"]
  APP -->|"pushUrl"| UVS["useViewState pushUrl loops {id}"]
  UVS -->|"pushState view=loops"| HASH["hash #/loops/{id}"]
  HASH --> UVS2["useViewState 解析 loopDetailId"]
  UVS2 --> APP2["App activeView=loops loopDetailId!=null"]
  APP2 --> LDP["LoopDetailPage loopId"]
```

## 数据结构图

```mermaid
classDiagram
  class LoopListPageProps {
    +onSelectLoop: (id: number) => void
    +onLoopChanged?: () => void
    +loopUpdateCount?: number
  }
  class View {
    loops: string
  }
  class NavOpts {
    +id?: number
  }
  class useViewState {
    +pushUrl(view: View, opts?: NavOpts): void
    +loopDetailId: number | null
  }
  class LoopDetailPageProps {
    +loopId: number
    +workspaceId?: number | null
    +onBack(): void
    +onLoopChanged(): void
  }
  LoopListPageProps --> View : onSelectLoop 触发 pushUrl loops
  useViewState --> LoopDetailPageProps : loopDetailId 注入 loopId
```

## 数据变更图

```mermaid
stateDiagram-v2
  [*] --> 列表态: /#/loops loopDetailId=null
  列表态 --> 路由变更: 用户点击行 pushUrl loops {id}
  路由变更 --> 详情态: hash 变为 #/loops/{id} useViewState 解析 loopDetailId
  详情态 --> 列表态: onBack replaceUrl loops 或浏览器后退
  详情态 --> 详情态: 从详情再 pushUrl loops {newId} 切换环路
  列表态 --> 列表态: 列表内搜索刷新等 不改 hash
```

## 开发指导

- **前端入口**：`frontend/src/components/loop-list/LoopListViewParts.tsx` 的 `buildOnRow`（整行 `onClick` → `onSelectLoop`）与名称列 `<a>`（`onClick` 内先 `e.stopPropagation()` 避免冒泡），`onSelectLoop` 由 `frontend/src/components/loop-list/index.tsx` 的 `LoopListPage` 透传，最终落到 `frontend/src/App.tsx` 的 `handleSelectLoop`（`clearSelection` + `pushUrl('loops', { id: loopId })`）。
- **后端入口**：本功能点不发请求，仅前端路由切换。详情态挂载后由 `LoopDetailPage` → `LoopDetailPanel.reload` 拉 `GET /api/v1/workspaces/{ws}/loops/{id}`。
- **注意**：名称列链接的 `onClick` 必须 `e.stopPropagation()`，否则会先触发行 `onRow.onClick`（两者都调 `onSelectLoop`，虽结果一致但会重复触发父组件 `clearSelection`/`pushUrl`）；`buildRowActions` 的菜单项也各自 `domEvent.stopPropagation()` 挡冒泡避免误跳详情。
- **扩展**：要改为携带 tab 预选（如直接打开执行历史），在 `NavOpts` 加 `tab` 字段，`pushUrl('loops', { id, tab })` 在 `useViewState` 拼 query 段，`LoopDetailPage` 解析后透传给 `LoopDetailPanel` 的 `Collapse defaultActiveKey`。
