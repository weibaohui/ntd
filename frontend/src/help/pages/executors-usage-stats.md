# AI 使用统计

## 功能位置
执行器页 →「执行器」Tab →「AI 使用统计」Card（右上角 `Switch` + cron 表达式配置）

## 数据流图（前端 → 后端）

```mermaid
flowchart LR
  ExecutorsPanel -->|"loadUsageStatsSettings<br>db.getUsageStatsSettings"| API1["GET /api/v1/usage-stats/settings"]
  API1 -->|"enabled/cron"| setState["setUsageStatsEnabled / setUsageStatsCron"]
  User["切换开关"] --> handleToggle["onChange<br>db.updateUsageStatsSettings(checked, cron)"]
  User["保存"] --> handleSaveUsageStats["handleSaveUsageStats<br>db.updateUsageStatsSettings(enabled, cron)"]
  handleToggle -->|"PUT"| API2["PUT /api/v1/usage-stats/settings"]
  handleSaveUsageStats --> API2
```

## 谑用关系链路图

```mermaid
flowchart TD
  ExecutorsPanel["ExecutorsPanel.tsx<br>ExecutorsPanel()"] --> loadUsageStatsSettings["loadUsageStatsSettings()<br>db.getUsageStatsSettings"]
  ExecutorsPanel --> UsageStatsCard["AI 使用统计 Card"]
  UsageStatsCard --> Switch["extra Switch<br>onChange"]
  Switch --> db1["db.updateUsageStatsSettings(checked, usageStatsCron)"]
  UsageStatsCard --> CronPresetSelect["CronPresetSelect<br>onChange setUsageStatsCron"]
  UsageStatsCard --> Cron["react-js-cron Cron<br>setValue cronTo6"]
  UsageStatsCard --> SaveButton["保存 Button"]
  SaveButton --> handleSaveUsageStats["handleSaveUsageStats"]
  handleSaveUsageStats --> db2["db.updateUsageStatsSettings(enabled, cron)"]
```

## 数据结构图

```mermaid
classDiagram
  class UsageStatsSettings {
    auto_usage_stats_enabled: boolean
    auto_usage_stats_cron: string
  }
```

## 数据变更图

```mermaid
stateDiagram-v2
  [*] --> Loading: useEffect mount
  Loading --> Disabled: enabled=false
  Loading --> Enabled: enabled=true
  Disabled --> Enabled: 切换开关
  Enabled --> Disabled: 切换开关
  Enabled --> Saving: 点击保存
  Saving --> Enabled: 保存完成
```

## 开发指导
- **前端入口**：`frontend/src/components/settings/ExecutorsPanel.tsx` 的 `loadUsageStatsSettings`、`handleSaveUsageStats` 回调；cron 用 `react-js-cron` + `cronTo5`/`cronTo6` 转换 5/6 段格式
- **后端入口**：`backend/src/handlers/usage_stats.rs` 处理 `GET /api/v1/usage-stats/settings`、`PUT /api/v1/usage-stats/settings`
- **注意**：cron 在前端用 5 段格式展示（`cronTo5`），存储用 6 段（`cronTo6`），因为后端 scheduler 依赖 6 段含秒的 cron
- **扩展**：新增统计维度（如按模型分组）需后端归档逻辑追加字段，前端 `UsageStatsSettings` 类型扩展对应键
