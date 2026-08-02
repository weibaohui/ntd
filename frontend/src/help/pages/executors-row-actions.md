# 执行器行操作

## 功能位置
执行器页 →「执行器」Tab → 配置表「操作」列（设为默认 / 检测 / 修复 / 安装 / 测试）

## 数据流图（前端 → 后端）

```mermaid
flowchart LR
  Row["操作列 render"] --> SetDefault["设为默认<br>db.setDefaultExecutor"]
  SetDefault -->|"PUT"| API1["PUT /api/v1/executors/{name}/default"]
  Row --> Detect["检测<br>db.detectExecutor"]
  Detect -->|"POST"| API2["POST /api/v1/executors/{name}/detect"]
  Row --> Repair["修复<br>db.repairExecutor"]
  Repair -->|"POST"| API3["POST /api/v1/executors/{name}/resolve"]
  Row --> Install["安装<br>InstallExecutorButton"]
  Row --> Test["测试<br>db.testExecutor"]
  Test -->|"POST"| API4["POST /api/v1/executors/{name}/test"]
```

## 谑用关系链路图

```mermaid
flowchart TD
  ExecutorsPanel["ExecutorsPanel.tsx<br>ExecutorsPanel()"] --> ActionColumn["Table columns render 操作"]
  ActionColumn --> SetDefaultBtn["设为默认 Button<br>onClick db.setDefaultExecutor"]
  SetDefaultBtn --> setDefaultExecutorCache["setDefaultExecutorCache(updated.name)"]
  SetDefaultBtn --> setExecutors["setExecutors map is_default"]
  ActionColumn --> DetectBtn["检测 Button<br>onClick db.detectExecutor"]
  DetectBtn --> setDetectResults["setDetectResults[name]"]
  ActionColumn --> RepairBtn["修复 Button<br>仅 !detectResult.found 显示"]
  RepairBtn --> db1["db.repairExecutor"]
  RepairBtn --> db2["db.updateExecutor(path, enabled:true)"]
  ActionColumn --> InstallBtn["InstallExecutorButton<br>仅 !detectResult.found 显示"]
  InstallBtn --> onInstalled["onInstalled<br>detect→repair→update"]
  ActionColumn --> TestBtn["测试 Button<br>onClick db.testExecutor"]
  TestBtn --> setTestModalData["setTestModalData<br>setTestModalVisible"]
```

## 数据结构图

```mermaid
classDiagram
  class ExecutorConfig {
    name: string
    display_name: string
    is_default: boolean
    path: string
    enabled: boolean
  }
  class DetectResult {
    binary_found: boolean
    path_resolved: string|null
  }
  class TestResult {
    test_passed: boolean
    output: string|null
    error: string|null
  }
```

## 数据变更图

```mermaid
stateDiagram-v2
  [*] --> Idle: 页面加载
  Idle --> SettingDefault: 点击设为默认
  SettingDefault --> Idle: 成功 setExecutors
  Idle --> Detecting: 点击检测
  Detecting --> Idle: 结果写入 detectResults
  Idle --> Repairing: 点击修复
  Repairing --> Idle: 修复完成更新路径
  Idle --> Installing: 点击安装
  Installing --> Idle: 安装完成 detect+repair+update
  Idle --> Testing: 点击测试
  Testing --> TestModalOpen: 结果弹窗展示
  TestModalOpen --> Idle: 关闭弹窗
```

## 开发指导
- **前端入口**：`frontend/src/components/settings/ExecutorsPanel.tsx` 的配置表 `columns` 中 `key: 'action'` 的 `render` 回调；`InstallExecutorButton` 来自 `settings/InstallExecutorButton.tsx`
- **后端入口**：`backend/src/handlers/executor_config.rs` 处理 `default`、`detect`、`resolve`、`test` 子路由
- **注意**：修复按钮仅在 `!detectResult?.found` 时显示；安装按钮仅在检测结果存在且未找到时显示；安装回调 `onInstalled` 用 `repair.path_resolved || detect.path_resolved || record.path` 三选一兜底路径
- **扩展**：新增行操作按钮在 `columns` 的操作列 `Space` 中追加 `Button`，并添加对应 loading state
