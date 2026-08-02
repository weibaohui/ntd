# 已安装技能总览

## 功能位置
技能页 →「总览」子视图（`Segmented` 默认值）

## 数据流图（前端 → 后端）

```mermaid
flowchart LR
  SkillsOverview["SkillsOverview.tsx<br>loadData()"] -->|"db.getSkillsList"| API1["GET /api/v1/skills"]
  API1 -->|"ExecutorSkills[]"| SkillsOverview
  SkillsOverview -->|"db.syncSkill"| API2["POST /api/v1/skills/sync"]
  SkillsOverview -->|"ImportExportModal"| Export["导出/导入"]
  Export -->|"db.exportSkill"| API3["GET /api/v1/skills/{executor}/{name}/export"]
  Export -->|"db.importSkill"| API4["POST /api/v1/skills/import"]
```

## 调用关系链路图

```mermaid
flowchart TD
  SkillsOverview["SkillsOverview.tsx<br>SkillsOverview()"] -->|"loadData"| dbGetSkillsList["db.getSkillsList"]
  SkillsOverview -->|"useEffect mount"| loadData["loadData()"]
  SkillsOverview --> Stats["useMemo stats<br>totalSkills/totalFiles/executorsWithSkills"]
  SkillsOverview --> allSkills["useMemo allSkills<br>按 filterExecutor 展平"]
  SkillsOverview --> filteredSkills["useMemo filteredSkills<br>按 searchText 过滤"]
  SkillsOverview --> executorTabs["useMemo executorTabs<br>各执行器技能计数"]
  SkillsOverview --> SkillCardView["skills/SkillCardView.tsx<br>卡片视图"]
  SkillsOverview --> SkillCard["内部 SkillCard<br>列表视图"]
  SkillsOverview --> SkillDetailDrawer["SkillDetailDrawer<br>详情抽屉"]
  SkillsOverview --> ImportExportModal["ImportExportModal<br>导入/导出弹窗"]
  SkillDetailDrawer -->|"db.getSkillContent"| getContent["GET /api/v1/skills/{executor}/{name}"]
  SkillDetailDrawer -->|"db.syncSkill"| syncAPI["POST /api/v1/skills/sync"]
```

## 数据结构图

```mermaid
classDiagram
  class ExecutorSkills {
    executor: string
    executor_label: string
    skills_dir: string
    skills_dir_exists: boolean
    skills: SkillMeta[]
  }
  class SkillMeta {
    name: string
    description: string
    version: string|null
    author: string|null
    license: string|null
    keywords: string[]
    file_count: number
    total_size: number
    modified_at: string|null
  }
  ExecutorSkills --> SkillMeta
```

## 数据变更图

```mermaid
stateDiagram-v2
  [*] --> Loading: useEffect mount
  Loading --> Loaded: getSkillsList 成功
  Loaded --> Loading: drawer onSyncSuccess/loadData
  Loaded --> Filtered: searchText/filterExecutor 变化
  Filtered --> Loaded: 清空筛选
  Loaded --> DrawerOpen: 点击技能卡 handleSkillClick
  DrawerOpen --> Loaded: 关闭抽屉
  Loaded --> ExportOpen: Dropdown 导入/导出
  ExportOpen --> Loaded: 弹窗关闭 loadData
```

## 开发指导
- **前端入口**：`frontend/src/components/skills/SkillsOverview.tsx` 的 `SkillsOverview` 组件，`loadData` 触发首次加载
- **后端入口**：`backend/src/handlers/skills.rs` 处理 `GET /api/v1/skills`、`POST /api/v1/skills/sync`
- **注意**：`stats.totalExecutors` 使用前端常量 `EXECUTORS.length` 而非 API 返回数量，因为 API 只返回有 skills 目录的执行器
- **扩展**：新增执行器时在 `frontend/src/types/execution.tsx` 的 `EXECUTORS` 数组追加条目，`EXECUTOR_COLORS` 映射色值即可
