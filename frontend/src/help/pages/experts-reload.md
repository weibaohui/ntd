# 重新加载专家

## 功能位置
专家页 → 页面卡片右上角「重新加载」按钮（Tooltip 提示「重新扫描 ~/.ntd/experts/ 目录」）

## 数据流图（前端 → 后端）

```mermaid
flowchart LR
  User["点击重新加载"] --> handleReload["handleReload"]
  handleReload -->|"db.reloadExperts"| API["POST /api/v1/experts/reload"]
  API -->|"LoadResult{loaded_count, errors}"| handleReload
  handleReload --> loadExperts["loadExperts()<br>db.getAllExperts"]
  loadExperts --> API2["GET /api/v1/experts"]
```

## 谑用关系链路图

```mermaid
flowchart TD
  ExpertsPanel["ExpertsPanel.tsx<br>ExpertsPanel()"] --> handleReload["handleReload<br>useCallback"]
  handleReload --> setReloading["setReloading(true)"]
  handleReload --> db1["db.reloadExperts()<br>POST /api/v1/experts/reload"]
  db1 --> messageCheck{errors.length > 0?}
  messageCheck -->|"是"| warningMsg["message.warning<br>loaded_count 成功/errors 失败"]
  messageCheck -->|"否"| successMsg["message.success<br>已重新加载 loaded_count 个"]
  handleReload --> loadExperts["loadExperts()<br>db.invalidateExpertCache + getAllExperts"]
```

## 数据结构图

```mermaid
classDiagram
  class LoadResult {
    loaded_count: number
    errors: string[]
  }
  class ExpertMetadata {
    name: string
    expert_type: string
    version: string
    display_name_zh: string
    display_name_en: string
    profession_zh: string
    description_zh: string
    avatar_path: string
    category_id: number
    tags: ExpertTag[]
    members: ExpertMember[]
    skills: SkillMetadata[]
    source: string
  }
```

## 数据变更图

```mermaid
stateDiagram-v2
  [*] --> Idle: 页面加载
  Idle --> Reloading: 点击重新加载
  Reloading --> Idle: reload 完成 loadExperts
  Reloading --> Idle: reload 失败
```

## 开发指导
- **前端入口**：`frontend/src/components/settings/ExpertsPanel.tsx` 的 `handleReload` 回调（`useCallback`）
- **后端入口**：`backend/src/handlers/experts.rs` 处理 `POST /api/v1/experts/reload`，清空内存索引并重新扫描 `~/.ntd/experts/`
- **注意**：`loadExperts` 开头调用 `db.invalidateExpertCache()` 失效单专家缓存，避免刷新后展示过期数据
- **扩展**：如需扫描额外目录，后端 reload handler 的扫描路径需对应修改
