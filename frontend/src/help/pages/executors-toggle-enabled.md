# 启用/停用执行器

## 功能位置
执行器页 →「执行器」Tab → 配置表「状态」列的 `Switch` 开关

## 数据流图（前端 → 后端）

```mermaid
flowchart LR
  User["切换 Switch"] -->|"onChange(checked)"| onSave["setSavingExecutor(name)"]
  onSave -->|"db.updateExecutor(name, enabled: checked)"| API["PUT /api/v1/executors/{name}"]
  API -->|"updated ExecutorConfig"| setExecutors["setExecutors map replace"]
```

## 谑用关系链路图

```mermaid
flowchart TD
  ExecutorsPanel["ExecutorsPanel.tsx<br>ExecutorsPanel()"] --> EnabledColumn["Table columns 状态列"]
  EnabledColumn --> Switch["antd Switch<br>checked=enabled<br>loading=savingExecutor===name"]
  Switch --> onChange["onChange async"]
  onChange --> setSavingExecutor["setSavingExecutor(record.name)"]
  onChange --> db1["db.updateExecutor(record.name, enabled: checked)"]
  db1 --> setExecutors["setExecutors prev.map replace"]
  onChange --> setError["message.error"]
  onChange --> setSavingExecutorNull["setSavingExecutor(null)"]
```

## 数据结构图

```mermaid
classDiagram
  class ExecutorConfig {
    name: string
    enabled: boolean
  }
```

## 数据变更图

```mermaid
stateDiagram-v2
  [*] --> Idle: 页面加载
  Idle --> Saving: 切换开关
  Saving --> Idle: update 成功 setExecutors
  Saving --> Idle: update 失败
```

## 开发指导
- **前端入口**：`frontend/src/components/settings/ExecutorsPanel.tsx` 的配置表 `columns` 中 `key: 'enabled'` 的 `Switch onChange` 回调（内联在 JSX render）
- **后端入口**：`backend/src/handlers/executor_config.rs` 处理 `PUT /api/v1/executors/{name}`，接收 `{ enabled: boolean }` partial update
- **注意**：关闭开关的执行器不会出现在 Todo 的执行器选择列表中；批量检测也会根据二进制可用性联动此开关
- **扩展**：如需在启用/停用时触发额外操作（如清理会话），后端 update handler 的 `enabled` 分支需追加逻辑
