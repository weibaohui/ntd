# Wiki 布区切换

## 功能位置

黑板页 → 左侧目录树 `Menu`（桌面端固定侧边栏 / 移动端 Drawer 内），点击目录项切换 Wiki 页面

## 数据流图（前端 → 后端）

```mermaid
flowchart LR
  U[用户点击目录项] --> MENU["Menu onClick key=slug"]
  MENU --> SS["handleSelectSlug(slug)"]
  SS --> CS["setCurrentSlug(slug)"]
  SS --> CD["setMenuDrawerOpen(false) 仅移动端"]
  SS --> RU["replaceUrl blackboard file=slug"]
  CS --> FCF["fetchCurrentFile 触发（依赖 currentSlug）"]
  FCF --> API["GET /api/v1/workspaces/{ws}/wiki/files/{slug}"]
  API --> H[get_wiki_file handler]
  H --> FS["读取 wiki 文件内容"]
  FS --> CONTENT[WikiFileContent]
  CONTENT --> SCF["setCurrentFile"]
  SCF --> BC[BlackboardContent 渲染 Markdown]
```

## 调用关系链路图

```mermaid
flowchart TD
  BlackboardPage --> handleSelectSlug["useCallback handleSelectSlug"]
  handleSelectSlug --> setCurrentSlug
  handleSelectSlug --> setMenuDrawerOpen["setMenuDrawerOpen(false) 移动端"]
  handleSelectSlug --> replaceUrl["replaceUrl blackboard file=slug"]
  setCurrentSlug --> useEffect_currentSlug["useEffect 依赖 currentSlug"]
  useEffect_currentSlug --> fetchCurrentFile["useCallback fetchCurrentFile"]
  fetchCurrentFile --> fetchWikiFileContent["fetchWikiFileContent(ws, slug)"]
  fetchWikiFileContent --> api_get["fetch GET /api/v1/workspaces/{ws}/wiki/files/{slug}"]
  api_get --> setCurrentFile
  setCurrentFile --> BlackboardWikiLayout
  BlackboardWikiLayout --> BlackboardContent["XMarkdown 渲染"]
  BlackboardContent --> TodoLink["TodoLink 解析 ntd:// 和 ./slug 链接"]
```

## 数据结构图

```mermaid
classDiagram
  class WikiFileItem {
    +slug: string
    +file_type: index_topic_log_string
  }
  class WikiFileContent {
    +slug: string
    +content: string
  }
  class BlackboardWikiLayoutProps {
    +isDark: boolean
    +isMobile: boolean
    +files: WikiFileItem[]
    +currentFile: WikiFileContent_null
    +currentSlug: string
    +onSelectSlug: fn
    +filesLoading: boolean
    +fileLoading: boolean
    +menuDrawerOpen: boolean
    +onMenuDrawerClose: fn
    +workspaceId: number
    +isTopic: boolean
    +onTopicDeleted: fn
  }
  BlackboardWikiLayout --> WikiFileItem
  BlackboardWikiLayout --> WikiFileContent
  BlackboardWikiLayout --> Menu["Ant Design Menu inline"]
```

## 数据变更图

```mermaid
stateDiagram-v2
  [*] --> Empty: currentSlug = '' 空初始态
  Empty --> Loading: fetchFiles 返回 → setCurrentSlug(defaultSlug)
  Loading --> Viewing: fetchCurrentFile 返回 → setCurrentFile
  Viewing --> Loading: 用户点击其他目录项 → setCurrentSlug(newSlug)
  Loading --> Viewing: 新文件内容返回
  Viewing --> Topic: 切换到 topic 类型页 → 渲染 TopicToolbar
  Viewing --> Log: 切换到 log 类型页 → 无工具条
  Topic --> Viewing: 切换到非 topic 页
  Loading --> RaceDrop: latestSlugRef 不匹配 → 丢弃
```

## 开发指导

- **前端入口**：`frontend/src/components/BlackboardPage.tsx` 的 `BlackboardWikiLayout` 组件和 `handleSelectSlug` 函数；Markdown 渲染在 `BlackboardContent` 组件
- **后端入口**：`backend/src/handlers/blackboard.rs` 的 `get_wiki_file` 和 `list_wiki_files` handler
- **注意**：`BlackboardWikiLayout` 用 `useMemo` 构造 `menuItems` 和 `sidebarContent`，避免每次父组件重渲染时重建数组触发 Ant Design Menu 内部 `prefixCls` null 崩溃；切换工作空间时不设 `currentSlug = ''`（会导致 Menu `selectedKeys={['']}` 崩溃），而是清空 `files` 让 Menu 不渲染，`fetchFiles` 完成后自动设回有效 slug；`TodoLink` 覆盖了 `a` 标签渲染，识别 `ntd://todo/{id}` 协议和 `./slug` Wiki 相对路径
- **扩展**：若需新增文件类型（如 `draft`），在 `WikiFileItem.file_type` 联合类型中追加，在 `menuItems` 构造逻辑中增加分组规则；若需支持文件重命名，在 `TopicToolbar` 追重命名操作并调用对应后端接口
