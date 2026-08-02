# 导入专家

## 功能位置
专家页 → 页面卡片右上角「导入」下拉按钮（上传 zip 包 / 从 WorkBuddy 导入 / 从本地目录导入）

## 数据流图（前端 → 后端）

```mermaid
flowchart LR
  User["点击导入下拉"] --> Dropdown["Dropdown importMenuItems"]
  Dropdown -->|"key=zip"| FileInput["fileInputRef.click"]
  FileInput --> handleFileImport["handleFileImport"]
  handleFileImport -->|"db.importExpert(file)"| API1["POST /api/v1/experts/import<br>multipart/form-data"]
  Dropdown -->|"key=workbuddy"| handleWorkbuddyImport["handleWorkbuddyImport"]
  handleWorkbuddyImport -->|"db.importFromWorkbuddy"| API2["POST /api/v1/experts/import-from-workbuddy"]
  Dropdown -->|"key=dir"| DirModal["目录导入弹窗"]
  DirModal --> handleDirImport["handleDirImport"]
  handleDirImport -->|"db.importExpertFromDirectory(path)"| API3["POST /api/v1/experts/import-from-directory"]
  API1 --> loadExperts["loadExperts()"]
  API2 --> loadExperts
  API3 --> loadExperts
```

## 谑用关系链路图

```mermaid
flowchart TD
  ExpertsPanel["ExpertsPanel.tsx<br>ExpertsPanel()"] --> importMenuItems["importMenuItems<br>MenuProps"]
  importMenuItems --> FileInput["input type=file<br>accept=.zip"]
  FileInput -->|"onChange"| handleFileImport["handleFileImport(e)"]
  handleFileImport --> db1["db.importExpert(file)<br>FormData append file"]
  importMenuItems --> handleWorkbuddyImport["handleWorkbuddyImport"]
  handleWorkbuddyImport --> db2["db.importFromWorkbuddy"]
  importMenuItems --> DirModal["Modal 从本地目录导入"]
  DirModal --> handleDirImport["handleDirImport"]
  handleDirImport --> db3["db.importExpertFromDirectory(dirImportPath)"]
  db1 --> loadExperts["loadExperts()"]
  db2 --> loadExperts
  db3 --> loadExperts
```

## 数据结构图

```mermaid
classDiagram
  class ImportResult {
    expert: ExpertMetadata|null
    errors: string[]
  }
  class WorkbuddyImportResult {
    imported_count: number
    skipped_count: number
    errors: string[]
  }
  class LoadResult {
    loaded_count: number
    errors: string[]
  }
```

## 数据变更图

```mermaid
stateDiagram-v2
  [*] --> Idle: 页面加载
  Idle --> Importing: 点击导入项
  Importing --> Idle: 导入完成 loadExperts
  Importing --> Idle: 导入失败
  Idle --> DirModalOpen: 点击「从本地目录导入」
  DirModalOpen --> Idle: 输入路径确认 handleDirImport
  DirModalOpen --> Idle: 取消
```

## 开发指导
- **前端入口**：`frontend/src/components/settings/ExpertsPanel.tsx` 的 `handleFileImport`、`handleWorkbuddyImport`、`handleDirImport` 回调
- **后端入口**：`backend/src/handlers/experts.rs` 处理 `POST /api/v1/experts/import`、`import-from-directory`、`import-from-workbuddy`
- **注意**：zip 导入不手动设 `Content-Type`，交给浏览器自动生成带 boundary 的 multipart 头；WorkBuddy 导入返回 `imported_count` / `skipped_count` / `errors` 三段统计需分别拼装提示
- **扩展**：新增导入来源时在 `importMenuItems` 数组追加菜单项并在 `ExpertsPanel` 添加对应 handler
