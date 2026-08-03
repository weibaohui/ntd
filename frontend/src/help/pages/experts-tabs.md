# 专家 / 专家团队分区切换

## 功能位置
专家页 → `Tabs`（defaultActiveKey="experts"，两个 Tab：专家 / 专家团队）

## 数据流图（前端 → 后端）

```mermaid
flowchart LR
  filteredExperts["filteredExperts"] --> individualExperts["useMemo individualExperts<br>filter expert_type=agent"]
  filteredExperts --> teamExperts["useMemo teamExperts<br>filter expert_type=team"]
  individualExperts --> Tab1["Tab key=experts<br>ExpertCard 网格"]
  teamExperts --> Tab2["Tab key=teams<br>TeamCard 网格"]
```

## 谑用关系链路图

```mermaid
flowchart TD
  ExpertsPanel["ExpertsPanel.tsx<br>ExpertsPanel()"] --> filteredExperts["useMemo filteredExperts<br>搜索过滤"]
  filteredExperts --> individualExperts["useMemo individualExperts<br>localeCompare 排序"]
  filteredExperts --> teamExperts["useMemo teamExperts<br>localeCompare 排序"]
  ExpertsPanel --> Tabs["antd Tabs"]
  Tabs --> Tab1["Tab key=experts<br>label: 专家 count"]
  Tabs --> Tab2["Tab key=teams<br>label: 专家团队 count"]
  Tab1 --> ExpertCard["experts/ExpertCard.tsx"]
  Tab2 --> TeamCard["experts/TeamCard.tsx"]
```

## 数据结构图

```mermaid
classDiagram
  class ExpertMetadata {
    name: string
    expert_type: string
  }
  note for ExpertMetadata "expert_type = 'agent' | 'team'"
```

## 数据变更图

```mermaid
stateDiagram-v2
  [*] --> ExpertsTab: defaultActiveKey=experts
  ExpertsTab --> TeamsTab: 点击「专家团队」
  TeamsTab --> ExpertsTab: 点击「专家」
```

## 开发指导
- **前端入口**：`frontend/src/components/settings/ExpertsPanel.tsx` 的 `Tabs items` 数组，`individualExperts` / `teamExperts` useMemo 分区
- **后端入口**：无后端调用，纯前端分区
- **注意**：Tab label 附实时计数（`individualExperts.length`），搜索过滤后计数同步刷新；空列表时展示 `Empty` 占位
- **扩展**：新增专家类型分区时追加 `useMemo` 过滤并增加对应 Tab item
