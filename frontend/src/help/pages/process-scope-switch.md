# 我的/模板视图切换

## 功能位置

工艺 → 搜索栏上方的 `Segmented` 控件（「我的」「模板」两段）

## 数据流图（前端 → 后端）

```mermaid
flowchart LR
  U[用户点击 Segmented] --> HSC["handleScopeChange(value)"]
  HSC --"localStorage.setItem"--> LS["持久化 SCOPE_STORAGE_KEY"]
  HSC --> SS["setScope(next)"]
  SS --"useEffect 依赖 scope"--> LOAD["load()"]
  LOAD --"getProcesses(scope === 'template')"--> API["/api/bundled/processes?is_system=<bool>"]
  API --> H["list_process_templates handler"]
  H --"list_process_templates(is_system)"--> DB[(process_templates 表)]
  DB --"is_system=true 系统模板"--> R1["模板视图卡片"]
  DB --"is_system=false 用户工艺"--> R2["我的视图卡片"]
```

## 调用关系链路图

```mermaid
flowchart TD
  SEG["Segmented onChange"] --> HSC["handleScopeChange"]
  HSC --> LS["localStorage SCOPE_STORAGE_KEY"]
  HSC --> SS["setScope"]
  SS --> UE["useEffect([scope]) → load()"]
  UE --> API["bundledApi.getProcesses(isSystem)"]
  API --> GET["GET /api/bundled/processes?is_system="]
  GET --> H["backend list_process_templates"]
  H --> Q["ListProcessTemplatesQuery.is_system: Option<bool>"]
  Q --> DB["db.list_process_templates(is_system)"]
  DB --> RI["readInitialScope 初始值从 localStorage"]
```

## 数据结构图

```mermaid
classDiagram
  class ProcessScope {
    +mine: 文案「我的」
    +template: 文案「模板」
  }
  class ListProcessTemplatesQuery {
    +is_system: Option~bool~
  }
  class ProcessTemplate {
    +id: i64
    +guid: String
    +name: String
    +display_name: String
    +complexity: String
    +version: String
    +is_system: bool
  }
  ProcessScope --> ListProcessTemplatesQuery: scope 转 is_system
  ListProcessTemplatesQuery --> ProcessTemplate: 过滤返回
```

## 数据变更图

```mermaid
stateDiagram-v2
  [*] --> mine: 默认 / localStorage 恢复
  mine --> template: 点 Segmented「模板」
  template --> mine: 点 Segmented「我的」
  mine --> mine: 搜索客户端过滤子集
  template --> template: 搜索客户端过滤子集
  mine --> [*]
  template --> [*]
```

## 开发指导

- **前端入口**：`frontend/src/components/ProcessPage.tsx` 的 `ProcessListView` 内 `handleScopeChange` + `readInitialScope`；`SCOPE_STORAGE_KEY` 常量定义在同文件
- **后端入口**：`backend/src/handlers/process.rs` 的 `list_process_templates` handler，路由 `GET /api/bundled/processes`
- **注意**：服务端按 `is_system` 过滤，不做全量拉取+客户端 filter；切换视图不清空 `searchText`，避免用户输入丢失；空态按视图给出不同引导（「我的」空引导创建，「模板」空引导同步）
- **扩展**：新增视图段（如「团队」）时，扩 `ProcessScope` 联合类型、Segmented options、`readInitialScope` 兼容解析、后端 `ListProcessTemplatesQuery` 增查询字段
