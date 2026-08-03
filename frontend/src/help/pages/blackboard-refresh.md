# 刷新黑板

## 功能位置

黑板页 → 顶部标题栏右侧的「刷新」按钮（`ReloadOutlined`），桌面端在 `DesktopHeaderExtra`、移动端在 `MobileHeaderExtra`

## 数据流图（前端 → 后端）

```mermaid
flowchart LR
  U[用户点击刷新按钮] --> HE["DesktopHeaderExtra / MobileHeaderExtra onRefresh"]
  HE --> HR["handleRefresh"]
  HR --> FF["fetchFiles()"]
  HR --> FCF["fetchCurrentFile()"]
  FF --> API1["GET /api/v1/workspaces/{ws}/wiki/files"]
  API1 --> H1[list_wiki_files handler]
  H1 --> FS1["扫描 wiki 目录"]
  FS1 --> LIST[文件列表]
  LIST --> SF["setFiles + setCurrentSlug"]
  FCF --> API2["GET /api/v1/workspaces/{ws}/wiki/files/{slug}"]
  API2 --> H2[get_wiki_file handler]
  H2 --> FS2["读取文件内容"]
  FS2 --> CONTENT[Markdown 内容]
  CONTENT --> SCF["setCurrentFile"]
```

## 调用关系链路图

```mermaid
flowchart TD
  BlackboardPage --> handleRefresh["useCallback handleRefresh"]
  handleRefresh --> fetchFiles["fetchFiles()"]
  handleRefresh --> fetchCurrentFile["fetchCurrentFile()"]
  fetchFiles --> fetchWikiFiles["fetchWikiFiles(workspaceId)"]
  fetchCurrentFile --> fetchWikiFileContent["fetchWikiFileContent(workspaceId, slug)"]
  fetchWikiFiles --> api1["fetch GET /api/v1/workspaces/{ws}/wiki/files"]
  fetchWikiFileContent --> api2["fetch GET /api/v1/workspaces/{ws}/wiki/files/{slug}"]
  api1 --> setFiles
  api1 --> setCurrentSlug
  api2 --> setCurrentFile
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
  class BlackboardData {
    +id: number
    +workspace_id: number
    +blackboard_debounce_secs: number
    +blackboard_debounce_count: number
    +wiki_prompt: string
    +wiki_timeout_secs: number
    +enabled: boolean
    +updated_at: string_null
  }
```

## 数据变更图

```mermaid
stateDiagram-v2
  [*] --> Idle: 页面已加载
  Idle --> Refreshing: 点击刷新按钮
  Refreshing --> FetchingFiles: fetchFiles 并发
  Refreshing --> FetchingContent: fetchCurrentFile 并发
  FetchingFiles --> FilesReady: setFiles + setFilesLoading false
  FetchingContent --> ContentReady: setCurrentFile + setFileLoading false
  FilesReady --> Idle: 两个请求都完成
  ContentReady --> Idle: 两个请求都完成
  FetchingFiles --> RaceDrop: latestWorkspaceIdRef 不匹配 → 丢弃
  FetchingContent --> RaceDrop2: latestWorkspaceIdRef / latestSlugRef 不匹配 → 丢弃
```

## 开发指导

- **前端入口**：`frontend/src/components/BlackboardPage.tsx` 的 `handleRefresh` 函数（`useCallback`），调用 `fetchFiles` 和 `fetchCurrentFile`
- **后端入口**：`backend/src/handlers/blackboard.rs` 的 `list_wiki_files` 和 `get_wiki_file` handler
- **注意**：`fetchFiles` 和 `fetchCurrentFile` 内部都有 `latestWorkspaceIdRef` / `latestSlugRef` 防切换竞态守卫——resolve 后与 ref 比对，不一致说明期间已切换工作空间或文件，晚到的响应直接丢弃；刷新不包含重拉配置（`fetchConfig`），配置只在挂载和保存设置时更新
- **扩展**：若刷新时需要同时重拉配置，在 `handleRefresh` 中追加 `fetchConfig()` 调用；新增刷新后的副作用（如 toast 提示）在 `handleRefresh` 的末尾追加
