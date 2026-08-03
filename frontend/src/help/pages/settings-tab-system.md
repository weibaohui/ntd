# 系统设置 Tab

## 功能位置
更多设置页 →「系统设置」Tab（默认 Tab，`SystemSettingsPanel`）

## 数据流图（前端 → 后端）

```mermaid
flowchart LR
  SettingsPage -->|"useEffect mount<br>db.getConfig"| API1["GET /api/v1/config"]
  API1 --> configForm["configForm.setFieldsValues<br>port/host/db_path/log_level/..."]
  User["点击保存配置"] --> handleSaveConfig["handleSaveConfig"]
  handleSaveConfig --> validate["configForm.validateFields"]
  validate --> merge["mergedConfig = currentConfig + values"]
  merge -->|"db.updateConfig(mergedConfig)"| API2["PUT /api/v1/config"]
  API2 --> configFormSet["configForm.setFieldsValues(mergedConfig)"]
```

## 调用关系链路图

```mermaid
flowchart TD
  SettingsPage["SettingsPage.tsx<br>SettingsPage()"] --> configForm["Form.useForm configForm"]
  SettingsPage --> useEffectMount["useEffect mount<br>db.getConfig"]
  useEffectMount --> configForm["configForm.setFieldsValues<br>+ slash_command_rules"]
  SettingsPage --> handleSaveConfig["handleSaveConfig"]
  handleSaveConfig --> validate["configForm.validateFields"]
  validate --> getCurrentConfig["db.getConfig()"]
  getCurrentConfig --> merge["mergedConfig<br>...currentConfig ...values"]
  merge --> slashRulesNorm["slash_command_rules 规范化<br>trim + 补 / 前缀 + 嚍重"]
  slashRulesNorm --> db1["db.updateConfig(mergedConfig)"]
  db1 --> messageSuccess["message.success"]
  SettingsPage --> SystemSettingsPanel["SystemSettingsPanel.tsx<br>configForm/handleSaveConfig 传入"]
  SystemSettingsPanel --> PortField["port InputNumber 1-65535"]
  SystemSettingsPanel --> HostField["host Input"]
  SystemSettingsPanel --> DbPathField["db_path Input"]
  SystemSettingsPanel --> LogLevelField["log_level Select DEBUG/INFO/WARN/ERROR"]
  SystemSettingsPanel --> TimezoneField["scheduler_default_timezone Select"]
```

## 数据结构图

```mermaid
classDiagram
  class Config {
    port: number
    host: string
    db_path: string
    log_level: string
    slash_command_rules: SlashCommandRule[]
    default_response_todo_id: number|null
    history_message_max_age_secs: number
    max_concurrent_todos: number
    execution_timeout_secs: number
    scheduler_default_timezone: string
    blackboard_debounce_secs: number
    blackboard_debounce_count: number
    wiki_prompt: string
  }
  class SlashCommandRule {
    slash_command: string
    todo_id: number
    enabled: boolean
  }
  Config --> SlashCommandRule
```

## 数据变更图

```mermaid
stateDiagram-v2
  [*] --> Loading: useEffect mount
  Loading --> Loaded: getConfig 成功 setFieldsValues
  Loaded --> Saving: 点击保存配置
  Saving --> Loaded: update 成功 setFieldsValues
  Saving --> Loaded: 校验失败/保存失败
```

## 开发指导
- **前端入口**：`frontend/src/components/SettingsPage.tsx` 的 `handleSaveConfig` 回调；`frontend/src/components/settings/SystemSettingsPanel.tsx` 承载表单字段
- **后端入口**：`backend/src/handlers/config.rs` 处理 `GET /api/v1/config`、`PUT /api/v1/config`
- **注意**：`handleSaveConfig` 做 `slash_command_rules` 规范化（trim + 补 `/` 前缀 + 嚍重检查），重复命令会 `message.error` 并阻断保存；`mergedConfig` 先取 `currentConfig` 再 spread `values`，保证未表单管理的字段不被清空
- **扩展**：新增配置字段时 `Config` 类型追加键，`SystemSettingsPanel` 添加 `Form.Item`，`handleSaveConfig` 的 mergedConfig 构造追加该键的处理
