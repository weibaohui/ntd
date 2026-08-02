# 搜索过滤

## 功能位置

事项列表页 → 顶部 header 的 `Input`（`SearchOutlined` 前缀，`placeholder="搜索标题或 Prompt"`，`data-testid="items-page-search"`，桌面端可见；移动端精简隐藏）。

## 数据流图（前端 → 后端）

```mermaid
flowchart LR
  U[输入搜索词] --> onSearch["TodoListHeader Input.onChange → setSearchKeyword"]
  onSearch --> KW["searchKeyword state 更新"]
  KW -->|"列表形态"| FE1["filterBySearchKeyword(items, kw)"]
  KW -->|"卡片形态"| FE2["TodoCenterCardView 内部同样按 title/prompt 过滤"]
  FE1 --> Match["items.filter: title.includes(kw) 或 prompt.includes(kw), 不区分大小写"]
  FE2 --> Match2["kw.trim().toLowerCase() 命中"]
  Match --> UI["TodoListView Table 重渲染"]
```

## 调用关系链路图

```mermaid
flowchart TD
  Input["TodoListHeader Input"] -->|"onChange e.target.value"| setSearchKeyword["TodoListPage setSearchKeyword"]
  setSearchKeyword --> filter["filterBySearchKeyword(listItems, searchKeyword)"]
  filter -->|"kw 为空"| all["返回原 items"]
  filter -->|"kw 非空"| lower["kw = keyword.trim().toLowerCase()"]
  lower --> iter["items.filter: title.toLowerCase().includes 或 prompt.toLowerCase().includes"]
```

## 数据结构图

```mermaid
classDiagram
class TodoCenterItem_search {
  +title: string
  +prompt: string
}
class filterBySearchKeyword {
  +items: TodoCenterItem[]
  +keyword: string
  +返回: TodoCenterItem[]
}
class searchKeyword_state {
  +值: string
  +trim: 前后空格
  +toLowerCase: 不区分大小写
}
filterBySearchKeyword --> TodoCenterItem_search : 命中 title 或 prompt
searchKeyword_state --> filterBySearchKeyword : 输入
```

## 数据变更图

```mermaid
stateDiagram-v2
  [*] --> 无搜索: searchKeyword=''
  无搜索 --> 有搜索: Input.onChange 输入字符
  有搜索 --> 过滤中: filterBySearchKeyword 运行
  过滤中 --> 命中: title 或 prompt includes(kw)
  过滤中 --> 未命中: 被过滤丢弃
  有搜索 --> 无搜索: Input allowClear 清空
```

## 开发指导

- **前端入口**：`frontend/src/components/todo-list/TodoListPage.tsx` 的 `filterBySearchKeyword`（L44-52）与 `TodoListPage` 主函数（`searchKeyword` state + `listItems` 派生）；渲染在 `frontend/src/components/todo-list/TodoListPageParts.tsx` 的 `TodoListHeader`（Input，桌面端）。
- **后端入口**：无。搜索为纯前端二次过滤，不调后端。后端 `get_todo_center` 的 `search` 查询参数由 `TodoCenterCardView` 内部使用（卡片形态走后端过滤），列表形态走前端过滤。
- **注意**：搜索词统一大小写不敏感（`toLowerCase()` 后 `includes`）；空词返回全部。卡片/列表两种形态共用同一个 `searchKeyword` state，透传给 `TodoCenterCardView`（后端 search）和 `TodoListView`（前端 filter）。移动端精简 header 不渲染搜索框。
- **扩展**：若需按 `tag_ids` 或 `executor` 过滤，在 `filterBySearchKeyword` 增判断分支；若改走后端搜索，把 `searchKeyword` 透传给 `db.getTodoCenter` 的 `search` 参数即可（后端已按 title/prompt 子串匹配）。
