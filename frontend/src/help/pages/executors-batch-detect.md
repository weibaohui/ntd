# 批量检测执行器

## 功能位置
执行器页 →「执行器」Tab → 配置表上方「批量检测」按钮（`SearchOutlined`）

## 数据流图（前端 → 后端）

```mermaid
flowchart LR
  User["点击批量检测"] --> Loop["遍历 executors"]
  Loop -->|"db.detectExecutor(ec.name)"| API["POST /api/v1/executors/{name}/detect"]
  API -->|"binary_found / path_resolved"| ResultCheck{binary_found?}
  ResultCheck -->|"true 且 !ec.enabled"| UpdateOn["db.updateExecutor(name, enabled:true)"]
  ResultCheck -->|"false 且 ec.enabled"| UpdateOff["db.updateExecutor(name, enabled:false)"]
  UpdateOn --> setExecutors["更新前端 executors"]
  UpdateOff --> setExecutors
```

## 调用关系链路图

```mermaid
flowchart TD
  ExecutorsPanel["ExecutorsPanel.tsx<br>ExecutorsPanel()"] --> ButtonOnClick["批量检测 onClick"]
  ButtonOnClick --> setBatchDetecting["setBatchDetecting(true)"]
  ButtonOnClick --> Loop["for of executors"]
  Loop --> db1["db.detectExecutor(ec.name)"]
  db1 --> setDetectResults["setDetectResults<br>更新该行检测结果"]
  db1 --> enabledBranch{binary_found?}
  enabledBranch -->|"true && !ec.enabled"| updateOn["db.updateExecutor(enabled:true)"]
  enabledBranch -->|"false && ec.enabled"| updateOff["db.updateExecutor(enabled:false)"]
  updateOn --> setExecutors["setExecutors map replace"]
  updateOff --> setExecutors
  Loop --> messageSuccess["message.success<br>availableCount/total"]
```

## 数据结构图

```mermaid
classDiagram
  class ExecutorConfig {
    id: number
    name: string
    path: string
    enabled: boolean
    display_name: string
    session_dir: string
    is_default: boolean
    default_model: string|null
    supports_models: boolean
  }
  class DetectResult {
    binary_found: boolean
    path_resolved: string|null
  }
```

## 数据变更图

```mermaid
stateDiagram-v2
  [*] --> Idle: 页面加载
  Idle --> BatchDetecting: 点击批量检测
  BatchDetecting --> Detecting: 遍历每个执行器
  Detecting --> BatchDetecting: 继续下一个
  BatchDetecting --> Idle: 遍历完成 message
  BatchDetecting --> Idle: 失败
```

## 开发指导
- **前端入口**：`frontend/src/components/settings/ExecutorsPanel.tsx` 的「批量检测」按钮 `onClick` 回调（内联在 JSX）
- **后端入口**：`backend/src/handlers/executor_config.rs` 处理 `POST /api/v1/executors/{name}/detect`（调 `which` 解析二进制路径）
- **注意**：批量检测会根据 `binary_found` 联动更新 `enabled` 开关——找到但未启用则自动启用，未找到但已启用则自动停用
- **扩展**：如需检测时执行额外操作（如写入检测时间戳），后端 detect handler 需追加逻辑并让前端在 `setDetectResults` 同步更新
