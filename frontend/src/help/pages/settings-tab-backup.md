# 备份与恢复 Tab

## 功能位置
更多设置页 →「备份与恢复」Tab（`BackupPanel`）

## 数据流图（前端 → 后端）

```mermaid
flowchart LR
  BackupPanel["BackupPanel.tsx"] -->|"handleTriggerBackup"| API1["POST /api/v1/backup/trigger"]
  BackupPanel -->|"handleDownloadDatabase"| API2["GET /api/v1/backup/download<br>responseType=blob"]
  BackupPanel -->|"handleOptimizeDatabase"| API3["POST /api/v1/backup/optimize"]
  BackupPanel -->|"handleSaveAutoBackup"| API4["PUT /api/v1/backup/auto-backup"]
  BackupPanel -->|"handleDeleteBackup"| API5["DELETE /api/v1/backup/files/{filename}"]
  BackupPanel -->|"handleTriggerTodoBackup"| API6["POST /api/v1/todo-backup/trigger"]
  BackupPanel -->|"handleTriggerSkillBackup"| API7["POST /api/v1/skill-backup/trigger"]
  BackupPanel -->|"handleExportBackup"| API8["GET /api/v1/backup/export"]
  BackupPanel -->|"handleImportFile"| API9["POST /api/v1/backup/import"]
```

## 谑用关系链路图

```mermaid
flowchart TD
  SettingsPage["SettingsPage.tsx<br>SettingsPage()"] --> BackupTab["Tab backup"]
  BackupTab --> BackupPanel["BackupPanel.tsx<br>BackupPanel()"]
  BackupPanel --> Tabs["内部 Tabs"]
  Tabs --> DatabaseBackupTab["backup/DatabaseBackupTab.tsx<br>数据库备份"]
  Tabs --> TodoBackupTab["backup/TodoBackupTab.tsx<br>事项备份"]
  Tabs --> SkillBackupTab["backup/SkillBackupTab.tsx<br>技能备份"]
  BackupPanel --> ImportExportModals["backup/ImportExportModals.tsx<br>导入导出向导"]
  ImportExportModals --> handleImportFile["handleImportFile"]
  handleImportFile --> handleWizardConfirm["handleWizardConfirm<br>逐项 overwrite/skip"]
  BackupPanel --> handleTriggerBackup["handleTriggerBackup"]
  BackupPanel --> handleDownloadDatabase["handleDownloadDatabase"]
  BackupPanel --> handleOptimizeDatabase["handleOptimizeDatabase"]
```

## 数据结构图

```mermaid
classDiagram
  class BackupDataYaml {
    todos: Todo[]
    tags: Tag[]
    experts: ExpertMetadata[]
    skills: SkillMeta[]
    workspaces: ProjectDirectory[]
  }
  class ImportItem {
    key: number
    type: string
    name: string
    action: overwrite|skip
  }
  BackupDataYAML --> ImportItem: ImportExportModals
```

## 数据变更图

```mermaid
stateDiagram-v2
  [*] --> Idle: Tab 加载
  Idle --> BackingUp: 触发备份
  BackingUp --> Idle: 备份完成 message
  Idle --> Downloading: 下载数据库
  Downloading --> Idle: blob URL 触发下载
  Idle --> Optimizing: 优化数据库
  Optimizing --> Idle: 优化完成
  Idle --> Importing: 导入文件
  Importing --> WizardOpen: 解析 BackupDataYAML
  WizardOpen --> Idle: 向导确认 handleWizardConfirm
```

## 开发指导
- **前端入口**：`frontend/src/components/settings/BackupPanel.tsx` 的 `BackupPanel` 组件；内部 Tabs 分数据库/事项/技能备份，导入导出由 `backup/ImportExportModals.tsx` 承载
- **后端入口**：`backend/src/handlers/backup.rs` 处理数据库备份/恢复/优化/导入导出；事项备份和技能备份见对应 handler
- **注意**：导入向导用 `ImportItem` 的 `overwrite` / `skip` 逐项控制冲突处理策略；`handleDownloadDatabase` 用 `responseType=blob` 接收二进制后 `URL.createObjectURL` 触发下载
- **扩展**：新增备份子类（如工艺模板备份）时内部 Tabs 追加子 Tab，后端追加对应 trigger/auto-backup/files 接口
