# 云端同步 Tab

## 功能位置
更多设置页 →「云端同步」Tab（`CloudSyncPanel`）

## 数据流图（前端 → 后端）

```mermaid
flowchart LR
  CloudSyncPanel["CloudSyncPanel.tsx"] -->|"useEffect mount<br>syncApi.getStatus"| API1["GET /api/v1/sync/status"]
  CloudSyncPanel --> -->|"handleSaveConfig<br>syncApi.updateConfig"| API2["PUT /api/v1/sync/config"]
  CloudSyncPanel -->|"handleSyncConfirm<br>syncApi.sync"| API3["POST /api/v1/sync"]
  API3 -->|"direction=push/pull"| API3
  CloudSyncPanel -->|"handleClearHistory"| API4["DELETE /api/v1/sync/history"]
```

## 谑用关系链路图

```mermaid
flowchart TD
  SettingsPage["SettingsPage.tsx<br>SettingsPage()"] --> CloudSyncTab["Tab cloudSync"]
  CloudSyncTab --> CloudSyncPanel["CloudSyncPanel.tsx<br>CloudSyncPanel()"]
  CloudSyncPanel --> useEffectMount["useEffect mount<br>syncApi.getStatus + syncApi.getConfig"]
  useEffectMount --> setStatus["setStatus"]
  useEffectMount --> setConfig["setConfig"]
  CloudSyncPanel --> ConfigForm["配置 Form<br>url/branch/auto_sync_enabled/auto_sync_cron"]
  ConfigForm --> handleSaveConfig["handleSaveConfig<br>syncApi.updateConfig"]
  CloudSyncPanel --> SyncModal["同步 Modal<br>direction=push/pull"]
  SyncModal --> handleSyncConfirm["handleSyncConfirm<br>syncApi.sync(direction)"]
  CloudSyncPanel --> HistoryTable["历史 Table"]
  CloudSyncPanel --> handleClearHistory["handleClearHistory"]
```

## 数据结构图

```mermaid
classDiagram
  class SyncConfig {
    url: string
    branch: string
    auto_sync_enabled: boolean
    auto_sync_cron: string
  }
  class SyncStatus {
    remote_url: string
    branch: string
    local_exists: boolean
    local_commit: string|null
    remote_commit: string|null
    needs_update: boolean|null
    last_sync_at: string|null
  }
```

## 数据变更图

```mermaid
stateDiagram-v2
  [*] --> Loading: useEffect mount
  Loading --> Loaded: getConfig/getStatus 完成
  Loaded --> Saving: 点击保存配置
  Saving --> Loaded: updateConfig 成功
  Loaded --> SyncModalOpen: 点击 push/pull
  SyncModalOpen --> Syncing: 确认同步
  Syncing --> Loaded: 同步完成 getStatus 刷新
  Loaded --> Idle: handleClearHistory
```

## 开发指导
- **前端入口**：`frontend/src/components/settings/CloudSyncPanel.tsx` 的 `CloudSyncPanel` 组件；`syncApi` 来自 `frontend/src/utils/database/sync`
- **后端入口**：`backend/src/handlers/sync.rs` 处理 `/api/v1/sync` 系列接口（status / config / sync / history）
- **注意**：同步方向分 `push`（本地→远程）和 `pull`（远程→本地），确认弹窗中展示方向语义；`needs_update` 为 null 表示尚未比较过本地远程 commit
- **扩展**：新增同步方向（如 `bidirectional`）后端 sync handler 追加分支，前端 `openSyncModal` 的 direction union 扩展
