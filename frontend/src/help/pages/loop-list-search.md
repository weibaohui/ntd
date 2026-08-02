# 搜索环路

## 功能位置

环路（列表） → 顶部 `PageCard` 的 `extra` 区 → `LoopListHeader` 搜索框（`Input` 带 `SearchOutlined` 前缀，`data-testid="loop-list-search"`）。

## 数据流图（前端 → 后端）

```mermaid
flowchart LR
  U[用户在搜索框输入关键词] --> IN["Input onChange → setSearchKeyword"]
  IN --> ST["useState searchKeyword"]
  ST --> FM["useMemo filteredItems 按 name 过滤"]
  FM --> LV[LoopListView 重渲染 table]
  LV --> DST[仅本地数据过滤 不发请求]
  DST --> R[用户看到匹配行]
```

## 调用关系链路图

```mermaid
flowchart TD
  LoopListHeader["LoopListHeader Input onChange"] -->|"onSearchChange(kw)"| LLP["LoopListPage setSearchKeyword"]
  LLP -->|"useState"| SK["searchKeyword state"]
  LLP -->|"useMemo filteredItems"| FM["items.filter name.includes kw.toLowerCase"]
  FM -->|"传 filteredItems"| LV["LoopListView dataSource"]
```

## 数据结构图

```mermaid
classDiagram
  class LoopListPage {
    +searchKeyword: string
    +setSearchKeyword(v): void
    +filteredItems: LoopListItem[]
  }
  class LoopListHeaderProps {
    +searchKeyword: string
    +onSearchChange: (kw: string) => void
  }
  class LoopListItem {
    +id: number
    +name: string
    +status: string
    +step_count: number
  }
  LoopListPage --> LoopListHeaderProps : props
  LoopListPage --> LoopListItem : items 过滤后传 LoopListView
```

## 数据变更图

```mermaid
stateDiagram-v2
  [*] --> 空关键词: 初次挂载 searchKeyword 为空
  空关键词 --> 输入中: 用户键入字符 onChange
  输入中 --> 过滤态: useMemo 用 kw.trim().toLowerCase 过滤 items.name
  过滤态 --> 输入中: 继续键入
  过滤态 --> 空关键词: 用户 allowClear 清空
  过滤态 --> 全匹配: kw 为空 返回 items 原集
```

## 开发指导

- **前端入口**：`frontend/src/components/loop-list/LoopListPageParts.tsx` 的 `LoopListHeader` 组件（`Input` `onChange`），关键词 state 与过滤在 `frontend/src/components/loop-list/index.tsx` 的 `LoopListPage`（`useState` + `useMemo`）。
- **后端入口**：无。搜索是纯前端对已拉取的 `items` 按 `name` 子串过滤，不发请求。
- **注意**：过滤口径是 `(l.name || '').toLowerCase().includes(kw.trim().toLowerCase())`，空关键词时直接返回原 `items`；`name` 可能为空字符串，用 `|| ''` 兜底避免 `toLowerCase` 报错。
- **扩展**：要改为服务端搜索，需在 `dbLoops.listLoops` 加查询参数并在后端 `list_loops_v1` 的 `list_loops_with_counts` 原生 SQL 增 `WHERE l.name LIKE ?` 分支；要按状态/标签过滤则在 `filteredItems` 的 `useMemo` 里加判定即可。
