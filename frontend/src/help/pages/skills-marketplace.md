# 技能市场

## 功能位置
技能页 →「技能市场」子视图（`Segmented` 第二项）

## 数据流图（前端 → 后端）

```mermaid
flowchart LR
  SkillMarketplace["SkillMarketplace.tsx"] -->|"loadSkills<br>bundledApi.getSkills"| API1["GET /api/bundled/skills?page&page_size&source&keyword"]
  SkillMarketplace -->|"loadSources<br>bundledApi.getSkillSources"| API2["GET /api/bundled/skill-sources?page&page_size&keyword"]
  SkillMarketplace -->|"handleCardClick<br>bundledApi.getSkillContent"| API3["GET /api/bundled/skills/{name}/content"]
  SkillMarketplace -->|"handleInstall<br>bundledApi.installSkill"| API4["POST /api/bundled/skills/install"]
  SkillMarketplace -->|"loadInstalled<br>db.getSkillsList"| API5["GET /api/v1/skills"]
```

## 调用关系链路图

```mermaid
flowchart TD
  SkillMarketplace["SkillMarketplace.tsx<br>SkillMarketplace()"] -->|"useEffect"| loadBranch{viewMode 分支}
  loadBranch -->|"browse-sources 且无 activeSource"| loadSources["loadSources()"]
  loadBranch -->|"其余"| loadSkills["loadSkills()"]
  loadBranch --> loadInstalled["loadInstalled()"]
  loadSources --> bundledApi1["bundledApi.getSkillSources"]
  loadSkills --> bundledApi2["bundledApi.getSkills"]
  loadInstalled --> dbGet["db.getSkillsList"]
  SkillMarketplace --> handleCardClick["handleCardClick<br>竞态守卫 detailReqIdRef"]
  handleCardClick --> bundledApi3["bundledApi.getSkillContent"]
  SkillMarketplace --> handleOpenInstall["handleOpenInstall"]
  handleOpenInstall --> InstallModal["安装 Modal"]
  InstallModal --> handleInstall["handleInstall<br>遍历 targetExecutors"]
  handleInstall --> bundledApi4["bundledApi.installSkill"]
  SkillMarketplace --> SkillFileBrowserModal["SkillFileBrowserModal"]
  SkillFileBrowserModal --> bundledApi5["bundledApi.getSkillFileContent"]
```

## 数据结构图

```mermaid
classDiagram
  class BundledSkillMeta {
    name: string
    short_name: string
    source: string
    source_meta: SkillSourceMeta
  }
  class SkillSourceMeta {
    name: string
    display_name: string
    description: string
    github_url: string
    stars: number
    license: string
    author: string
  }
  class SkillSourceWithCount {
    source: string
    meta: SkillSourceMeta
    skill_count: number
  }
  BundledSkillMeta --> SkillSourceMeta
```

## 数据变更图

```mermaid
stateDiagram-v2
  [*] --> BrowseSources: 默认 viewMode
  BrowseSources --> AllSkills: switchToAllSkills
  AllSkills --> BrowseSources: switchToSourceBrowse
  BrowseSources --> SourceDetail: enterSource(sourceKey)
  SourceDetail --> BrowseSources: 返回来源网格
  AllSkills --> DrawerOpen: handleCardClick
  SourceDetail --> DrawerOpen: handleCardClick
  DrawerOpen --> InstallModal: handleOpenInstall
  InstallModal --> BrowseSources: 安装完成 loadInstalled
  AllSkills --> AllSkills: 翻页 setAllPage
  BrowseSources --> BrowseSources: 翻页 setBrowseSourcesPage
```

## 开发指导
- **前端入口**：`frontend/src/components/skills/SkillMarketplace.tsx` 的 `SkillMarketplace` 组件
- **后端入口**：`backend/src/handlers/bundled.rs` 处理 `/api/bundled/skills` 系列接口
- **注意**：`loadSkills` 与 `loadSources` 共用 `reqGenRef` 做竞态守卫，快速翻页/切换视图时过期请求静默丢弃；`handleCardClick` 用独立的 `detailReqIdRef` 防止详情内容覆盖
- **扩展**：新增来源只需在 `~/.ntd/bundled/skills/` 下放置来源目录及 `metadata.json`，后端扫描即自动识别；前端 `ALL_SKILLS_PAGE_SIZE` 控制每页条数（默认 30）
