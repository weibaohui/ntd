# 搜索过滤

## 功能位置

事项列表页 → 顶部 header 的 `Input`（`SearchOutlined` 前缀，`placeholder="搜索标题或 Prompt"`，`data-testid="items-page-search"`，桌面端可见；移动端精简隐藏）。

## 数据流图（前端 → 后端）

```mermaid
flowchart LR
  U[输入搜索词] --> onSearch["TodoListHeader Input.onChange → setSearchKeyword"]
  onSearch --> KW["searchKeyword state 更新"]
  KW --> Debounce["SEARCH_DEBOUNCE_MS 防抖（056）<br/>停顿后才落盘 debouncedSearch 并重回第 1 页"]
  Debounce -->|"列表形态（056 起下推服务端）"| API["getTodoCenter(ws, search=kw)<br/>服务端 WHERE title/prompt LIKE + 分页"]
  KW -->|"卡片形态"| FE2["TodoCenterCardView 内部过滤"]
  API --> UI["TodoListView Table 渲染当前页"]
  FE2 --> Match2["kw.trim().toLowerCase() 命中"]
```

## 调用关系链路图

```mermaid
flowchart TD
  Input["TodoListHeader Input"] -->|"onChange e.target.value"| setSearchKeyword["TodoListPage setSearchKeyword"]
  setSearchKeyword --> debounce["useEffect setTimeout<br/>SEARCH_DEBOUNCE_MS 防抖"]
  debounce -->|"停顿后"| settled["setDebouncedSearch(kw.trim())<br/>+ setPage(1) 重回首页"]
  settled --> reload["useTodoListData.reload useCallback<br/>deps: page/pageSize/debouncedSearch/sort"]
  reload --> api["db.getTodoCenter(ws, {<br/>page, pageSize, search, sortBy, sortOrder})"]
  api --> server["服务端 WHERE title/prompt LIKE<br/>+ ORDER BY + LIMIT/OFFSET"]
  server --> render["setItems(data.items) + setTotal(data.total)<br/>TodoListView 渲染当前页"]
```

## 数据结构图

```mermaid
classDiagram
class TodoCenterItem_search {
  +title: string
  +prompt: string
}
class GetTodoCenterParams {
  +page: number
  +pageSize: number
  +search?: string «防抖后下推»
  +sortBy?: string
  +sortOrder?: asc|desc
}
class TodoCenterPage {
  +items: TodoCenterItem[]
  +total: number
  +bucket_counts: Record «应用 search 后、应用 bucket 前»
}
class searchKeyword_state {
  +searchKeyword: string «即时值，输入框受控»
  +debouncedSearch: string «防抖后值，驱动请求»
}
searchKeyword_state --> GetTodoCenterParams : debouncedSearch → search
GetTodoCenterParams --> TodoCenterPage : 服务端分页响应
```

## 数据变更图

```mermaid
stateDiagram-v2
  [*] --> 无搜索: searchKeyword=''
  无搜索 --> 输入中: Input.onChange 输入字符
  输入中 --> 输入中: 继续输入（防抖计时重置）
  输入中 --> 服务端查询: 停顿 SEARCH_DEBOUNCE_MS<br/>debouncedSearch 落盘 + page=1
  服务端查询 --> 展示当前页: getTodoCenter 返回 items+total
  输入中 --> 无搜索: Input allowClear 清空（防抖后 search=undefined）
  note right of 服务端查询: 搜索在服务端执行
  前端不再全量 includes 过滤
```

## 开发指导

- **前端入口**：`frontend/src/components/todo-list/TodoListPage.tsx` 的 `useTodoListData`——`searchKeyword` 即时值供输入框受控，`debouncedSearch` 经 `SEARCH_DEBOUNCE_MS` 防抖后作为 `search` 参数调 `db.getTodoCenter`，同时 `setPage(1)` 重回第 1 页（新搜索结果从首页看起）；渲染输入框在 `TodoListPageParts.tsx` 的 `TodoListHeader`（桌面端）。
- **后端入口**：`GET /api/v1/workspaces/{ws}/todos/center` 的 `search` 查询参数（`handlers/todo.rs::get_todo_center_v1` → `db/todo.rs::get_todo_center`），按 title/prompt 子串匹配；056 起列表形态与卡片形态都走该参数，无前端过滤残留。
- **注意**：防抖期间输入框即时回显但不发请求；防抖落盘即重回第 1 页，避免停在高页码看不到新结果；`bucket_counts`（五类驱动计数）是应用 search 之后、应用 bucket 之前统计的，搜索会同步收窄各分类计数。移动端精简 header 不渲染搜索框。
- **扩展**：新增筛选维度（如 tag/executor）时在 `getTodoCenter` 参数与后端查询条件同步加字段，并接入 `useTodoListData` 的 reload 依赖；改防抖时长调 `SEARCH_DEBOUNCE_MS` 常量。
