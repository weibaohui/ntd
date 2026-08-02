# 版本更新

## 功能位置
技能页 →「版本更新」子视图（`Segmented` 第三项）

## 数据流图（前端 → 后端）

```mermaid
flowchart LR
  SkillVersionUpdate["SkillVersionUpdate.tsx<br>loadData()"] -->|"db.getSkillVersionUpdates"| API1["GET /api/v1/skills/version-update"]
  SkillVersionUpdate -->|"db.getSkillsComparison"| API2["GET /api/v1/skills/compare"]
  SkillVersionUpdate -->|"handleConfirmUpdate<br>db.syncSkill"| API3["POST /api/v1/skills/sync"]
```

## 调用关系链路图

```mermaid
flowchart TD
  SkillVersionUpdate["SkillVersionUpdate.tsx<br>SkillVersionUpdate()"] --> loadData["loadData()<br>Promise.all[updates, comparisons]"]
  loadData --> db1["db.getSkillVersionUpdates"]
  loadData --> db2["db.getSkillsComparison"]
  SkillVersionUpdate --> allSkills["useMemo allSkills<br>从 comparisonData 构建同版本项"]
  allSkills --> buildSameVersionUpdate["buildSameVersionUpdate(comparison)"]
  SkillVersionUpdate --> filteredUpdates["useMemo filteredUpdates<br>按 searchText 过滤有差异项"]
  SkillVersionUpdate --> filteredAllSkills["useMemo filteredAllSkills"]
  SkillVersionUpdate --> SkillVersionCard["SkillVersionCard<br>有差异技能卡"]
  SkillVersionUpdate --> Collapse["Collapse 折叠<br>同版本技能区"]
  SkillVersionCard --> handleUpdateClick["handleUpdateClick<br>打开确认弹窗"]
  handleUpdateClick --> ConfirmModal["确认更新 Modal"]
  ConfirmModal --> handleConfirmUpdate["handleConfirmUpdate"]
  handleConfirmUpdate --> db3["db.syncSkill<br>source_executor + target_executors"]
  SkillVersionCard --> handleSkillClick["handleSkillClick<br>打开详情 Drawer"]
  handleSkillClick --> SkillDetailDrawer["SkillDetailDrawer"]
```

## 数据结构图

```mermaid
classDiagram
  class SkillVersionUpdate {
    skill_name: string
    description: string
    versions: SkillVersionInfo[]
    latest_version: string|null
    latest_executor: string
    has_update: boolean
  }
  class SkillVersionInfo {
    executor: string
    executor_label: string
    version: string|null
    modified_at: string|null
    is_latest: boolean
  }
  class SkillComparison {
    skill_name: string
    description: string
    executors: Record~string, SkillPresence~
  }
  class SkillPresence {
    present: boolean
    version: string|null
    modified_at: string|null
  }
  SkillVersionUpdate --> SkillVersionInfo
```

## 数据变更图

```mermaid
stateDiagram-v2
  [*] --> Loading: useEffect mount
  Loading --> Loaded: loadData 成功
  Loaded --> Filtered: searchText 变化
  Filtered --> Loaded: 清空搜索
  Loaded --> ConfirmOpen: handleUpdateClick
  ConfirmOpen --> Updating: handleConfirmUpdate
  Updating --> Loaded: syncSkill 成功 loadData
  Updating --> ConfirmOpen: sync 失败
  Loaded --> DrawerOpen: handleSkillClick
  DrawerOpen --> Loaded: 关闭抽屉
```

## 开发指导
- **前端入口**：`frontend/src/components/skills/SkillVersionUpdate.tsx` 的 `SkillVersionUpdate` 组件，`buildSameVersionUpdate` 将无差异的 `SkillComparison` 转为展示用的 `SkillVersionUpdate` 结构
- **后端入口**：`backend/src/handlers/skills.rs` 处理 `GET /api/v1/skills/version-update`、`GET /api/v1/skills/compare`、`POST /api/v1/skills/sync`
- **注意**：`handleConfirmUpdate` 只同步 `!v.is_latest` 的执行器，从 `latest_executor` 复制到 `targetExecutors`，会覆盖目标执行器同名 skill
- **扩展**：新增版本比对维度需同时改 `SkillComparison.executors` 映射键和 `buildSameVersionUpdate` 的判断逻辑
