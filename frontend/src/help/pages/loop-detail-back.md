# 返回列表

## 功能位置

环路（详情） → `LoopDetailPage` 的 `PageCard` 页头 extra 区最右端 → 「返回列表」按钮（`ArrowLeftOutlined`）。062 起统一走 `PageCard` 的 `onBack` prop（固定右上角锡点），各详情页不再手写 `titleSuffix` 按钮。

## 数据流图（前端 → 后端）

```mermaid
flowchart LR
  U[用户点击返回列表] --> OB["onBack 回调"]
  OB --> APP["App replaceUrl loops 或 history.back"]
  APP --> URL["hash 路由 #/loops"]
  URL --> UVS["useViewState 解析 loopDetailId=null"]
  UVS --> APP2["App activeView=loops loopDetailId=null"]
  APP2 --> LLP["渲染 LoopListPage 列表态"]
  LLP --> LD["useLoopListData 按 loopUpdateCount 重拉"]
```

## 调用关系链路图

```mermaid
flowchart TD
  TS["PageCard onBack（extra 最右端，062）"] -->|"onBack"| LDP["LoopDetailPage onBack"]
  LDP -->|"App 注入"| APP["App.tsx onBack 回调"]
  APP -->|"replaceUrl"| UVS["useViewState replaceUrl loops"]
  UVS -->|"pushState view=loops"| HASH["hash #/loops"]
  HASH --> APP2["App activeView=loops loopDetailId=null"]
  APP2 --> LLP["LoopListPage"]
```

## 数据结构图

```mermaid
classDiagram
  class LoopDetailPageProps {
    +loopId: number
    +onBack(): void
    +onLoopChanged(): void
  }
  class useViewState {
    +replaceUrl(view: View, opts?: NavOpts): void
    +loopDetailId: number | null
  }
  class LoopListPageProps {
    +onSelectLoop: (id: number) => void
    +loopUpdateCount?: number
  }
  LoopDetailPageProps --> useViewState : onBack 触发 replaceUrl loops
  useViewState --> LoopListPageProps : loopDetailId=null 时挂载列表页
```

## 数据变更图

```mermaid
stateDiagram-v2
  [*] --> 详情态: /#/loops/{id} loopDetailId!=null
  详情态 --> 列表态: 点击返回列表 replaceUrl loops
  列表态 --> 列表态: 列表内搜索/刷新 不改 hash
  列表态 --> 详情态: 用户再点某行 pushUrl loops {id}
  详情态 --> 详情态: 删除成功后 handleDeleteWithBack 调 onBack
```

## 开发指导

- **前端入口**：`frontend/src/components/LoopDetailPage.tsx` 的 `LoopDetailPage`（062 起将 `onBack` 传给 `PageCard` 的 `onBack` prop，按钮由 PageCard 统一渲染在 extra 区最右端），`onBack` 由 `frontend/src/App.tsx` 注入（`replaceUrl('loops')` 让 `loopDetailId` 解析为 `null`）。删除流程里 `handleDeleteWithBack` 也会在删除成功后调 `onBack` 自动回到列表。
- **后端入口**：无。纯前端路由切换，不落环路后端接口。
- **注意**：`handleDeleteWithBack` 用 `useCallback([handleDelete, onBack])` 稳定引用，避免每次重渲染创建新函数传给 `LoopDetailPanel`/`onActionsReady` effect 触发死循环（NTD-007 引用链稳定性设计）；返回列表后 `LoopListPage` 通过监听 `loopUpdateCount` 自动重拉，能反映详情页的删除/启停结果。
- **扩展**：要支持「返回到列表并保持滚动位置/选中态」，需在 `replaceUrl('loops')` 前把列表 `scrollTop`/`selectedIds` 存到 `App` 的 ref/state，并在 `LoopListPage` 挂载时恢复；当前实现是纯重挂载，列表分页/滚动会复位。
