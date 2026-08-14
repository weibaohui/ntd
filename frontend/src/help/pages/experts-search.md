# 搜索专家

## 功能位置
专家页 → 搜索栏（`Input` 前缀 `SearchOutlined`，placeholder「搜索专家名称、职业或描述」），搜索框左侧为来源筛选 Segmented（全部/我的/内置）

## 数据流图（前端 → 后端）

```mermaid
flowchart LR
  User["输入搜索词"] --> setSearchText["setSearchText"]
  User["切来源筛选"] --> setSourceFilter["setSourceFilter"]
  searchText --> filteredExperts["useMemo filteredExperts"]
  sourceFilter --> filteredExperts["先按来源过滤，再按关键词过滤（AND）"]
  filteredExperts --> individualExperts["useMemo individualExperts<br>filter expert_type=agent + 排序"]
  filteredExperts --> teamExperts["useMemo teamExperts<br>filter expert_type=team + 排序"]
```

## 谑用关系链路图

```mermaid
flowchart TD
  ExpertsPanel["ExpertsPanel.tsx<br>ExpertsPanel()"] --> searchTextState["useState searchText"]
  searchTextState --> filteredExperts["useMemo filteredExperts<br>name/profession/description includes keyword"]
  filteredExperts --> individualExperts["useMemo individualExperts<br>localeCompare zh-CN"]
  filteredExperts --> teamExperts["useMemo teamExperts<br>localeCompare zh-CN"]
  individualExperts --> ExpertsTab["Tabs 专家 Tab"]
  teamExperts --> TeamsTab["Tabs 专家团队 Tab"]
```

## 数据结构图

```mermaid
classDiagram
  class ExpertMetadata {
    name: string
    expert_type: string
    display_name_zh: string
    profession_zh: string
    description_zh: string
  }
  note for ExpertMetadata "getExpertDisplayName / getExpertProfession / getExpertDescription<br>从双语字段取展示值"
```

## 数据变更图

```mermaid
stateDiagram-v2
  [*] --> All: searchText 为空
  All --> Filtered: 输入关键词
  Filtered --> All: 清空搜索
  Filtered --> Filtered: 续输关键词
```

## 开发指导
- **前端入口**：`frontend/src/components/settings/ExpertsPanel.tsx` 的 `filteredExperts` useMemo（`useMemo` 依赖 `experts`、`searchText` 与 `sourceFilter`）
- **后端入口**：无后端调用，纯前端从已加载列表过滤
- **注意**：搜索关键词统一 `toLowerCase()` 后做 `includes`；排序用 `localeCompare(nameB, 'zh-CN')` 稳定排序避免刷新时顺序跳动
- **扩展**：新增搜索维度时在 `filteredExperts` 的 `filter` 回调追加字段判断
