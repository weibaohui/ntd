# 专家详情 Modal

## 功能位置
专家页 → 点击专家卡 / 团队卡 → `ExpertDetailModal` 弹窗

## 数据流图（前端 → 后端）

```mermaid
flowchart LR
  User["点击专家卡"] --> handleOpenDetail["handleOpenDetail"]
  handleOpenDetail -->|"Promise.all"| Load1["db.getExpertAgentMd(name)"]
  handleOpenDetail -->|"Promise.all"| Load2["db.getExpertSkills(name)"]
  Load1 -->|"GET"| API1["GET /api/v1/experts/{name}/agent-md"]
  Load2 -->|"GET"| API2["GET /api/v1/experts/{name}/skills"]
  DetailModal["ExpertDetailModal"] -->|"onExport"| handleExport["handleExport<br>db.exportExpert"]
  handleExport --> API3["GET /api/v1/experts/{name}/export<br>responseType=blob"]
  DetailModal -->|"onDelete"| handleDelete["handleDelete<br>db.deleteExpert"]
  handleDelete --> API4["DELETE /api/v1/experts/{name}"]
```

## 谑用关系链路图

```mermaid
flowchart TD
  ExpertsPanel["ExpertsPanel.tsx<br>ExpertsPanel()"] --> handleOpenDetail["handleOpenDetail<br>useCallback"]
  handleOpenDetail --> setSelectedExpert["setSelectedExpert<br>setDetailOpen(true)"]
  handleOpenDetail --> PromiseAll["Promise.all<br>getExpertAgentMd + getExpertSkills"]
  ExpertsPanel --> ExpertDetailModal["experts/ExpertDetailModal.tsx"]
  ExpertDetailModal --> HeaderSection["头部:头像/名称/版本/职业/分类"]
  ExpertDetailModal --> TagsSection["标签列表"]
  ExpertDetailModal --> MembersSection["团队成员 MemberItem<br>仅 team 类型"]
  ExpertDetailModal --> SkillsSection["关联技能列表"]
  ExpertDetailModal --> AgentMdSection["Agent MD 预览"]
  ExpertDetailModal --> handleDeleteClick["handleDeleteClick<br>打开删除确认 Modal"]
  handleDeleteClick --> handleConfirmDelete["handleConfirmDelete<br>onDelete"]
```

## 数据结构图

```mermaid
classDiagram
  class ExpertMetadata {
    name: string
    expert_type: string
    version: string
    display_name_zh: string
    profession_zh: string
    description_zh: string
    avatar_path: string
    category_id: number
    tags: ExpertTag[]
    members: ExpertMember[]
    skills: SkillMetadata[]
  }
  class SkillMetadata {
    skill_name: string
    skill_dir: string
    yaml_name: string
    yaml_description_zh: string
    yaml_emoji: string
  }
  class ExpertMember {
    id: string
    name_zh: string
    profession_zh: string
    role: string
  }
  ExpertMetadata --> SkillMetadata
  ExpertMetadata --> ExpertMember
```

## 数据变更图

```mermaid
stateDiagram-v2
  [*] --> Closed: 详情未打开
  Closed --> Loading: handleOpenDetail
  Loading --> Open: agentMd/skills 加载完成
  Open --> Closed: 关闭 Modal handleCloseDetail
  Open --> DeleteConfirm: 点击删除
  DeleteConfirm --> Open: 取消
  DeleteConfirm --> Deleting: 确认删除
  Deleting --> Closed: 删除成功 loadExperts
```

## 开开发指导
- **前端入口**：`frontend/src/components/settings/experts/ExpertDetailModal.tsx` 的 `ExpertDetailModal` 组件；由 `ExpertsPanel` 的 `handleOpenDetail` / `handleExport` / `handleDelete` 驱动
- **后端入口**：`backend/src/handlers/experts.rs` 处理 `GET /api/v1/experts/{name}/agent-md`、`GET /api/v1/experts/{name}/skills`、`GET /api/v1/experts/{name}/export`、`DELETE /api/v1/experts/{name}`
- **注意**：`ExpertDetailModal` 内的 `useState` / `useIsMobile` Hooks 必须在 `if (!expert) return null` 之前调用，否则 expert 从 null→对象时 Hook 数量变化导致崩溃
- **扩展**：新增详情区块（如「配置预览」）在 Modal 内容区追加 JSX 并在 `handleOpenDetail` 预加载对应数据
