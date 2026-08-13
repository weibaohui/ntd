# 模板管理 Tab

## 功能位置
更多设置页 →「模板管理」Tab（`TemplatesPanel`）

## 数据流图（前端 → 后端）

```mermaid
flowchart LR
  TemplatesPanel["TemplatesPanel.tsx"] -->|"bundledApi.getStatus"| API1["GET /api/v1/bundled/status?subdir"]
  TemplatesPanel -->|"bundledApi.sync"| API2["POST /api/v1/bundled/sync"]
  TemplatesPanel -->|"bundledApi.getConfig"| API3["GET /api/v1/bundled/config"]
  TemplatesPanel -->|"bundledApi.updateConfig"| API4["PUT /api/v1/bundled/config"]
  TemplatesPanel --> ExpertsTemplatesTab["templates/ExpertsTemplatesTab"]
  TemplatesPanel --> TodoTemplatesTab["templates/TodoTemplatesTab"]
  TemplatesPanel --> SkillTemplatesTab["templates/SkillTemplatesTab"]
  TemplatesPanel --> ProcessTemplatesTab["templates/ProcessTemplatesTab"]
```

## 调用关系链路图

```mermaid
flowchart TD
  SettingsPage["SettingsPage.tsx<br>SettingsPage()"] --> TemplatesTab["Tab templates"]
  TemplatesTab --> TemplatesPanel["TemplatesPanel.tsx<br>TemplatesPanel()"]
  TemplatesPanel --> Tabs["内部 Tabs 分子页"]
  Tabs --> ExpertsTab["ExpertsTemplatesTab<br>专家模板"]
  Tabs --> TodoTab["TodoTemplatesTab<br>事项模板"]
  Tabs --> SkillTab["SkillTemplatesTab<br>技能模板"]
  Tabs --> ProcessTab["ProcessTemplatesTab<br>工艺模板"]
  TemplatesPanel --> handleSync["handleSync<br>bundledApi.sync"]
  handleSync --> messageReport["message.success/error"]
  TemplatesPanel --> ConfigModal["ConfigModal<br>远程仓库配置"]
  TemplatesPanel --> StatusModal["StatusModal<br>同步状态展示"]
  TemplatesPanel --> InstallGitButton["InstallGitButton<br>未装 git 时展示"]
```

## 数据结构图

```mermaid
classDiagram
  class BundledConfig {
    url: string
    branch: string
    local_path: string
    auto_sync_enabled: boolean
    auto_sync_cron: string
    last_sync_at: string|null
  }
  class BundledStatus {
    remote_url: string
    branch: string
    local_exists: boolean
    local_commit: string|null
    remote_commit: string|null
    needs_update: boolean|null
    last_sync_at: string|null
    subdir: string
    subdir_exists: boolean
    subdir_file_count: number
    git_available: boolean
  }
  class SyncResult {
    success: boolean
    message: string
    is_first_clone: boolean
    has_updates: boolean
    changed_files: number
    subdir: string
  }
```

## 数据变更图

```mermaid
stateDiagram-v2
  [*] --> Idle: Tab 加载
  Idle --> Syncing: 点击同步 handleSync
  Syncing --> Idle: 同步完成 message
  Idle --> ConfigOpen: 点击配置
  ConfigOpen --> Idle: 保存配置 bundledApi.updateConfig
  Idle --> StatusOpen: 点击查看状态
  StatusOpen --> Idle: 关闭弹窗
```

## 开发指导
- **前端入口**：`frontend/src/components/settings/TemplatesPanel.tsx` 的 `TemplatesPanel` 组件；内部 Tabs 分专家/事项/技能/工艺模板，各子 Tab 由 `templates/` 下独立组件承载
- **后端入口**：`backend/src/handlers/bundled.rs` 处理 `/api/v1/bundled/*` 系列接口
- **注意**：同步前需检查 `git_available`，未安装 git 时展示 `InstallGitButton`；`handleSync` 的结果用 `SyncResult.is_first_clone` 区分首次克隆与后续 fetch
- **扩展**：新增模板子目录类型时 `Subdir` union 追加值，`TemplatesPanel` 内部 Tabs 新增对应子 Tab 组件
