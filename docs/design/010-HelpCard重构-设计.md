# 飞书 /help 任务控制台 · 重设计文档（v2）

> 状态：设计稿，待梳理确认。基于代码调研 

 


## 一、菜单设计

### 一级菜单：4 个 Tab


#### Tab 1「事项」
分页列出事项，当前智能体绑定了哪个工作空间，就显示哪个工作空间的事项

| 二级项 | 显示 | 交互 | 点击执行 |
|--------|------|------|----------|
|事项标题|---按钮---|---act---|---执行-------|

#### Tab 2「环路」
分页列出环路，当前智能体绑定了哪个工作空间，就显示哪个工作空间的环路

| 二级项 | 显示 | 交互 | 点击执行 |
|--------|------|------|----------|
|环路标题|---按钮---|---act---|---执行-------|


#### Tab 3「工作空间」

| 二级项 | 显示 | 交互 | 点击执行 |
|--------|------|------|----------|
| 当前工作空间 | markdown：工作空间名 + 默认执行器（来自 `workspace_settings`） | 只读 | — |
| 工作空间列表 | list_item：每个工作空间 + [切换]（当前项标 ▶ primary） | act | `act:/bind <workspace_id>` → `update_agent_bot_workspace_id` + 级联，patch 刷新 |
| 设定推送目标 | 按钮 [📍 设为推送目标] | act | `act:/sethome` → `set_feishu_home` + 开启响应 |
| 推送全部事件 | 按钮 [全部推送] （当前项高亮） | act | `act:/push all` → `update_feishu_push_level` |
| 仅推送结论 | 按钮 [仅推送结论] | act | `act:/push result` → `update_feishu_push_level` |
| 关闭推送 | 按钮 [关闭推送] | act | `act:/push off` → `update_feishu_push_level` |

> 无「解绑」按钮 —— bot 必须有工作空间，只能切换。


#### Tab 4「状态」（默认）

| 二级项 | 显示 | 交互 | 点击执行 |
|--------|------|------|----------|
| 状态条 | markdown：📌 当前工作空间 / ▶ 运行状态 / 🔔 推送级别 | 只读 | — |
| 主操作 | 按钮 [🆕 新会话] [⏹ 停止] | act | `act:/new` 清 session；`act:/stop` cancel |
| 历史入口 | 按钮 [查看全部历史 →] | nav | `nav:/history` patch 历史页 |
| 最近任务 | markdown 列表（5 条：状态 emoji + 标题 + 时间） | 只读 | — |



#### 历史子页（`nav:/history`）

| 二级项 | 显示 | 交互 | 点击执行 |
|--------|------|------|----------|
| 记录列表 | markdown 分页（状态 emoji + 触发类型 + 时间） | 只读 | — |
| 分页 | [← 上一页][返回控制台][下一页] → | nav | `nav:/history <page>` patch |

### action 前缀（沿用 cc-connect 约定）
- `nav:` 只读 patch 刷新（切 Tab、历史）
- `act:` 执行副作用 + patch 刷新（new / stop / bind 切 workspace / push / sethome）
- `cmd:` 异步发消息（保留，help 卡片主用 act）
- select 选中 → option 回传 → 统一走 act 分支（channel.rs 已加 fallback）

### 交互模式
- 点按钮 → 卡片**原地 patch 刷新**（顶部 ✅/⚠️ 操作结果提示），不发新消息
- `act:/stop` 显示「停止中」，最终 ❌ 状态由 `FeishuPushService` 推送通道发新卡片（cancel 是异步信号）



## 二、核心概念（调研结论）

| 概念 | 实体 | 说明 |
|------|------|------|
| 工作空间 | `project_directories` 表 | `workspace_id` = `project_directories.id` |
| 智能体 | `agent_bots` 表 | 有 `workspace_id` 字段，1 个 bot 属于 1 个 workspace |
| 切换工作空间 | 改 `agent_bot.workspace_id` | DB：`update_agent_bot_workspace_id`；REST：`PUT /api/agent-bots/{id}/workspace`（handler `move_bot_to_workspace`，带级联） |

`move_bot_to_workspace` 的级联逻辑（`backend/src/handlers/agent_bot.rs:886`）：
1. pending binding（`__pending__`）直接删除
2. 已生效 binding 设 `enabled=false`（保留记录）
3. `update_agent_bot_workspace_id(bot_id, workspace_id)`
4. bot 在运行则重启 listener

**消息执行有两条互斥路径**（`feishu_listener.rs:225` handle_message）：
- **binding 路径**（阶段5 `try_route_project_binding`）：该 chat 有 enabled 的 `feishu_project_binding` → 用 `binding.project_dir_id` + `binding.todo_id`（有 session resume）
- **default_response 路径**（阶段6 `route_slash_or_default_response`）：无 binding → 用 `agent_bot.workspace_id` → `workspace_settings.default_response_executor`（如 pi）

> **关键**：切换 `agent_bot.workspace_id` 只影响 default_response 路径（下次消息即生效）。

## 三、bind 语义重定义

| | 旧 bind | 新 bind |
|---|---|---|
| 做什么 | 绑 chat+project+**建 todo** | **切换 agent_bot.workspace_id** |
| 调用 | `create_feishu_project_binding` + `create_todo_with_extras` | `update_agent_bot_workspace_id`（+ 级联，复用 `move_bot_to_workspace` 逻辑） |
| 执行器 | `todo.executor`（默认 claudecode） | `workspace_settings.default_response_executor`（pi） |
| unbind | `delete_feishu_project_binding` | **移除**（bot 必须有 workspace，只能切换不能解绑） |
 
## 四、待你梳理确认的问题

1. **切换工作空间后，该 chat 的旧 binding 怎么办？**
   - 对齐 `move_bot_to_workspace`：该 chat 之后走 default_response（新 workspace）

2. **最近任务 / 历史按什么维度？**
    
   - 按 `agent_bot.workspace_id`（切工作空间后看新工作空间的任务），更符合「工作空间」语义

3. **「工作空间列表」展示哪些？**
   - 所有 `project_directories`

4. **任务页状态条「当前工作空间」** 
    改用 `agent_bot.workspace_id`

5. **binding 路径（阶段5）整体要不要保留？**
    - 一个 bot 一个工作空间、chat 内对话都在这个工作空间」，原binding 路径废弃
