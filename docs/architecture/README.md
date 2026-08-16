# ntd 系统架构重新推导（基于代码）

> 本文档基于对 ntd 仓库代码的重新阅读，推导系统真实运行架构。配套 5 张架构框图（HTML/SVG）位于同目录 `docs/architecture/`。

## 变更记录

| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| AtomCode (GLM-5.2) | 2026-08-01 | 初始版本：基于代码重新推导系统架构 |

---

## 1. 配套架构图索引

| 编号 | 文件 | 内容 |
|------|------|------|
| 01 | `01-functional-arch.html` | 功能架构图：前端 SPA / 后端引擎 / 外部与存储 |
| 02 | `02-data-arch.html` | 数据架构图：SQLite 30+ 表，按业务域分组 |
| 03 | `03-module-relations.html` | 功能模块关联关系图：调用/数据/事件三类关系 |
| 04 | `04-loop-execution-flow.html` | 环路（Loop）执行链路框图：触发→主循环→PhaseDriver→终止 |
| 05 | `05-todo-execution-flow.html` | 事项任务（Todo）执行链路框图：触发→三阶段→spawn→完成 |

所有框图均为自包含 HTML（内联 SVG），支持深/浅主题切换，浏览器直接打开即可。

---

## 2. 功能架构（重新推导）

ntd 是一个 AI 驱动的任务引擎，采用 Rust 后端 + React 前端 + 多执行器 + 飞书集成的架构。

### 2.1 前端层（React SPA）

| 模块 | 职责 |
|------|------|
| Dashboard | 概览与统计卡片 |
| Todo 中心 | 事项任务列表/详情/看板 |
| Loop Studio | 环路编排与执行可视化 |
| 工艺编辑器 | Monaco YAML + React Flow 可视化 |
| 消息监控 | 飞书消息流监听与回复 |
| 设置页 | 执行器/工作空间配置 |

前端通过 `apiClient`（fetch + WebSocket）与后端通信。

### 2.2 后端层（Rust axum）

| 模块 | 职责 |
|------|------|
| axum HTTP | REST API 入口 |
| WebSocket | 事件广播通道 |
| Handlers | todo/loop/process/action 请求处理 |
| Services 层 | 业务逻辑编排 |
| DB 抽象层 | migration（v1-v83）+ entity（30+ 表）|

核心引擎模块：

| 模块 | 职责 |
|------|------|
| Executor Service | 三阶段执行编排（prepare→spawn→dispatch） |
| LoopRunner | 环路主循环 + 续跑 + 返工检测 |
| TodoScheduler | tokio-cron 定时触发环路 |
| 工艺服务 | phase_driver + gate_evaluator + transition_resolver |
| 黑板服务 | debouncer 合并写入 + wiki 文件存储 |

### 2.3 外部层与存储

| 模块 | 职责 |
|------|------|
| AI 执行器 | claude/codex/atomcode 等 13+ 种子进程 |
| 专家系统 | WorkBuddy 兼容的 plugin.json + MD 文件格式 |
| 飞书 IM | WebSocket + HTTP + Token 管理的消息通道 |
| Webhook 入口 | 外部事件触发执行 |
| Git 同步 | 从远程仓库拉取内置专家/模板/Skills |
| SQLite | todos/loops/process_templates/execution_records 等 30+ 表 |

---

## 3. 数据架构（重新推导）

数据库为 SQLite，通过 migration v1-v83 演进，entity 层 30+ 张表。按业务域分组：

### 3.1 域 1：事项任务（Todo）

- **todos 表**（主表）：id, title, prompt, status, executor, workspace_id, schedule, acceptance_criteria, expert_name, created_at, updated_at, archived_at
  - TodoStatus 枚举：pending/running/completed/failed/pending_approval
  - smart_create 走 `default_response_todo_id`
- **todo_templates 表**：Todo 模板（id, name, prompt, executor, default_params, workspace_id）

### 3.2 域 2：环路执行（Loop）

- **loops 表**（主表）：id, name, status(enabled/disabled), workspace_id, workspace_path, limits_config
- **loop_steps 表**：环路环节（id, loop_id, todo_id, order_index, on_success, on_rating_fail, gate_config, expected_artifacts, skill_names, max_rework）
- **loop_executions 表**：环路执行实例（id, loop_id, trigger_type, trigger_meta, status, total_steps, started_at, finished_at）
- **loop_step_executions 表**：环节执行记录（id, loop_execution_id, step_id, todo_id, status, sequence_index, rework_count）
- **loop_step_execution_gates 表**：门禁评价记录（id, step_execution_id, gate_type, name, status, result）

### 3.3 域 3：工艺模板（Process）

- **process_templates 表**（主表）：guid, name, display_name, category, version, source(system/user), yaml_content, is_system, created_at
- **loop_phases 表**：阶段定义（id, loop_id, name, order_index）
- **loop_phase_executions 表**：阶段执行记录（id, loop_execution_id, phase_id, status, started_at, finished_at）
- **loop_step_artifacts 表**：环节产物捕获（id, step_execution_id, spec_name, path, content_hash, captured_at）

### 3.4 域 4：执行记录与外部集成

- **execution_records 表**（核心）：id, todo_id, task_id, record_id, status, result, rating, usage(tokens), session_id, agent_runs, started_at, finished_at
- **execution_logs 表**：执行日志（id, record_id, stream_type, content, sequence, created_at）
- **feishu_* 系列表**（6 张）：feishu_messages, feishu_history_chats, feishu_group_whitelist, feishu_push_targets, feishu_project_bindings, feishu_response_config
- **workspace_* 表**：project_directories, workspace_settings, workspace_slash_commands, review_templates
- **其他辅助表**：usage_stats, agent_bots, quick_buttons, usage_executor_daily, usage_model_breakdown, sync_records, executors

### 3.5 跨域外键引用

- loop_steps.todo_id → todos.id
- loop_steps 通过 template_guid → process_templates
- loop_executions.loop_id → loops.id
- loop_step_execution_gates.loop_step_execution_id → loop_step_executions.id
- loop_step_artifacts.loop_step_execution_id → loop_step_executions.id
- loop_step_artifacts.execution_record_id → execution_records.id

---

## 4. 功能模块关联关系

模块间存在三类关系：

### 4.1 调用关系（绿色实线）

- 前端页面 → apiClient → 后端 Handlers
- TodoHandler/LoopHandler/ExecutionHandler → ExecutorService / LoopRunner
- ExecutorService → spawn 子进程（AI 执行器）
- LoopRunner → PhaseDriver → BlackboardService
- TodoScheduler → LoopRunner（定时触发）

### 4.2 数据访问关系（紫色实线）

- 所有 Service 通过 DB 抽象层读写 SQLite
- ExecutorService 落库 execution_records / execution_logs
- LoopRunner 落库 loop_executions / loop_step_executions
- BlackboardService 通过 debouncer 合并写入 wiki 文件

### 4.3 事件通知关系（黄色虚线）

- WebSocket 事件推送（前端订阅）
- 飞书事件 → 触发执行
- webhook 事件 → 触发执行
- LoopFinished 事件 → FeishuPushService 按 workspace 推送

---

## 5. 环路（Loop）执行链路（完整梳理）

### 5.1 触发与启动阶段

1. **手动触发** `dispatch_manual_with_meta`：trigger_id 为 None（044 后无 trigger 表），所有 status=enabled 的 loop 都允许手动触发
2. **创建 loop_execution**：status=running, total_steps=0
3. **加载 enabled steps**：`list_enabled_loop_steps_by_loop`
4. **校验 workspace 一致性**：`check_workspace_consistency`（所有环节 todo 必须与 loop 同 workspace）
5. **解析 trigger_meta**：提取 params + requirement
6. **初始化计数器**：completed/failed/total_executed/total_tokens_used

### 5.2 主循环阶段（`run_inner_from`）

每个环节执行以下步骤：

1. **全局限制检查**：
   - `total_executed >= max_executions` → finish with "capped_step" + trigger_abnormal_handler
   - `total_tokens_used >= max_total_tokens` → finish with "capped_token" + trigger_abnormal_handler
2. **死循环检测**：连续 5 次执行同一 step → abort
3. **构造 enhanced_prompt**：
   - `build_enhanced_prompt_with_requirement`：注入黑板变量 + 上一环节输出 + 需求
   - `inject_workspace_prompt`：注入工作空间级共识 prompt
   - todo.prompt 是只读模板，仅在内存中替换，绝不写回 DB
4. **创建 step_execution**：`create_loop_step_execution` + `mark_step_execution_started`
5. **spawn 子进程执行**：`start_step_todo_with_prompt`
6. **等待执行完成**：`wait_for_step_finish`
7. **PhaseDriver 委托**（has_process_config 时）：
   - 产物捕获：`capture_step_artifacts`
   - 门禁评价：`evaluate_step_gates`
   - 流转解析：`transition_resolver.resolve_next`
   - 返工统计：`rework_tracker.evaluate_rework`
   - 确定最终状态：success/failed/pending_approval
   - 更新 phase 生命周期
8. **黑板更新**：`blackboard_debouncer.push_pending_record`（debouncer 合并写入）
9. **确定下一步**：`current_idx = resolve_next(...)`

### 5.3 门禁类型（gate_config）

- **ai_criteria_review**：AI 评分门禁，min_score 阈值
- **human_approval**：人工审批门禁，pending → 审批推进 passed/failed
- 废弃类型 artifact_present / script_check 已统一并入 ai_criteria_review（046）

### 5.4 流转策略

- `next` / `skip` → 推进到下一索引
- `end` / `break` → 终止
- 裸 step_id（goto 跳转）：用 `success_goto_step_id` / `fail_goto_step_id` 流转（037 清除 `goto:` 前缀）

### 5.5 返工判定

- 门禁失败后 goto 上游 → 视为返工 → `rework_count` 递增
- 超过 `max_rework` → 强制失败，工艺终止
- 判定规则：`target_idx <= current_idx` → 返工

### 5.6 评分等待

- ai_criteria_review 门禁存在时，按 `timeout_secs` 等评分写回
- null=默认 300s，0=一直等，N=等 N 秒
- 避免无评分误判 fail（047）

### 5.7 终止与异常处理阶段

1. **计算最终 status**：success（failed==0）/ failed（completed==0）/ partial
2. **finish_loop_execution**：落库 final_status + completed + failed
3. **触发异常处理 Todo**（failed/partial 时）：`trigger_abnormal_handler`
4. **同步 task 状态**：`sync_task_status`（NTD-005）
5. **广播 LoopFinished 事件**：FeishuPushService 按 workspace 配置推送
6. **或发送飞书结果**（有 feishu_receive_id 时）：`send_result_to_feishu`

### 5.8 异常路径

- 超限/超时 → finish with capped_step/capped_token → trigger_abnormal_handler
- 死循环检测 → abort
- step start 失败 → finish_step_execution("failed") + increment failed
- run 失败 → finish_loop_execution("failed") + trigger_abnormal_handler + ReviewStatusChanged 事件

---

## 6. 事项任务（Todo）执行链路（完整梳理）

### 6.1 触发入口阶段

事项任务有多个触发入口，最终都收敛到 `start_todo_execution` → `ExecutorService`：

1. **execute_action**：action 触发，构造 RunTodoExecutionRequest 直接执行
2. **smart_create**：默认响应 Todo，通过 workspace_settings.default_response_todo_id 找到 todo
3. **Todo CLI**：`ntd todo execute`，通过 CLI 子命令触发

共同流程：
- `find_or_create_todo`：查找/创建 todo + workspace
- `replace_placeholders`：params 替换 prompt 占位符
- `start_todo_execution`：进入 ExecutorService

### 6.2 ExecutorService 三阶段编排

#### Stage 1: `prepare_execution_state`

1. **占位符替换**：`substitute_message_placeholders`
2. **register_task_and_load_todo**：注册 task（task_id + guard + cancel_rx）+ 加载 todo
3. **read_runtime_config**：max_concurrent / timeout_secs
4. **enforce_concurrency_limit**：并发检查（超限返回错误）
5. **注入环节级上下文**：`inject_step_context`（需求 054：期望产物 + spec 模板）
6. **注入工作空间级共识 prompt**：`inject_workspace_prompt`（需求 022）
7. **注入专家上下文**：`inject_expert_context`（Agent MD + 技能信息）
8. **注入工作空间运行背景**：`inject_workspace_background`（需求 042）
9. **select_executor_and_build_command**：选定 executor + 构造 command_args
10. **create_run_execution_record**：创建 execution record 落库

注入层次（由外到内）：工作空间运行背景 → 专家上下文 → 工作空间共识 → 环节期望产物 → 原任务

#### Stage 2: `start_todo_and_prepare_spawn`

1. **resolve_worktree_context**：解析 worktree 路径
2. **record_worktree_path**：记录 worktree 路径到 DB
3. **start_todo_or_cleanup**：启动 todo 或清理失败
4. **register_websocket_task_info**：注册 WebSocket 任务信息
5. **计算 effective_workspace_path**：worktree 路径 > todo.workspace_path > request.workspace_path

#### Stage 3: `dispatch_spawned_executor_task`

1. **tokio::spawn**：异步执行 spawn 闭包
2. 返回 ExecutionResult（task_id + record_id）

### 6.3 spawn 子进程执行阶段

`run_spawned_executor_task` 编排流程：

1. **emit_started_event**：WebSocket 广播 Started 事件
2. **try_spawn_executor_child**：启动子进程
3. **save_child_pid_and_close_stdin**：记录 PID + 关闭 stdin
4. **setup_log_capture_pipeline**：stdout/stderr 捕获 + log flusher + flush timer
5. **await_run_outcome_with_timeout**：等待子进程完成（超时 / 取消 / 完成）
6. **dispatch_outcome**：分发结果

### 6.4 完成与产物阶段

1. **persist_completion**：`finalize_normal_completion` 持久化完成记录
2. **提取 conclusion**：`extract_conclusion` 从 execution_record 提取结论
3. **blackboard_debouncer**：`push_pending_record` 推送到黑板 debouncer
4. **触发 hooks**：`fire_post_completion_hooks`
5. **更新 todo 状态**：`sync_task_status`
6. **执行完成**：LoopFinished 广播 / 飞书推送

### 6.5 数据落库

- **execution_records 表**：执行记录主表（status/result/rating/usage/session_id/agent_runs）
- **execution_logs 表**：stdout/stderr 流式捕获（stream_type/sequence/content）
- **todos 表**：todo 状态同步（status: pending→running→completed/failed）

### 6.6 异常路径

- 并发超限 → 错误返回（enforce_concurrency_limit）
- 取消/超时 → `run_cancellation_path` / `run_timeout_path`
- spawn 失败 → 错误返回

---

## 7. 推导依据（代码引用）

本架构推导基于以下代码文件：

### 后端入口与路由

- `backend/src/main.rs`：CLI 命令枚举（Commands），Server/Todo/Loop/Process/Stats/Daemon/Skills 子命令
- `backend/src/lib.rs`：模块导出（adapters/cli/config/daemon/db/executor_service/...）
- `backend/src/handlers/mod.rs`：Router 路由注册（1125 行），mount_v1_domain_routes

### 环路执行链路

- `backend/src/services/loop_trigger.rs`：LoopTriggerDispatcher，dispatch_manual_with_meta（044 后仅手动触发）
- `backend/src/services/loop_runner.rs`：LoopRunner（2446 行），spawn_run/run_inner/run_inner_from
- `backend/src/services/process/phase_driver.rs`：execute_step（产物捕获→门禁评价→流转解析→返工统计）
- `backend/src/services/process/gate_evaluator.rs`：evaluate_step_gates（门禁评价）
- `backend/src/services/process/transition_resolver.rs`：resolve_next（流转解析）
- `backend/src/services/process/rework_tracker.rs`：evaluate_rework（返工统计）

### 事项任务执行链路

- `backend/src/executor_service/mod.rs`：run_todo_execution / run_todo_execution_with_params
- `backend/src/executor_service/stages.rs`：三阶段编排（prepare_execution_state / start_todo_and_prepare_spawn / dispatch_spawned_executor_task）
- `backend/src/executor_service/spawn_lifecycle.rs`：run_spawned_executor_task（810 行）
- `backend/src/handlers/execution.rs`：start_todo_execution / smart_create_handler / resume_execution_handler
- `backend/src/handlers/action.rs`：execute_action

### 数据层

- `backend/src/db/`：migration（v1-v83）+ entity（30+ 表）
- `backend/src/db/entity/`：所有实体定义（todos/loops/loop_steps/loop_executions/...）

### 调度器

- `backend/src/scheduler.rs`：TodoScheduler（1480 行），tokio-cron 定时触发
- `backend/src/task_manager.rs`：TaskManager + TaskGuard

---

## 8. 已知限制与待改进点

1. **archify renderer 布局严格**：architecture 渲染器的 clean-flow/edge-through-node 检查非常严格，手调坐标成本高。本次 5 张图全部采用 hand-placed SVG fallback，精确控制布局。
2. **架构图未含完整连线**：为避免穿透，部分次要连线省略。完整模块依赖关系见代码引用部分。
3. **数据架构图未含所有 30+ 表**：按业务域分组后，部分辅助表（usage_stats/agent_bots/quick_buttons 等）合并展示。
4. **执行链路框图未含并发竞态**：环路续跑（resume）与并发执行的竞态条件未在框图中体现。

---

## 9. 与需求的对应关系

| 需求 | 实现 |
|------|------|
| 新建分支 | `docs/arch-rededuce` 分支 |
| 根据代码重新推导系统架构 | 见本文档第 2-4 节 + 架构图 01-03 |
| 画详细、符合真实情况的系统运行架构图 | 见架构图 01-03（功能/数据/关联） |
| 架构图包含功能架构 | 见 01-functional-arch.html |
| 架构图包含数据架构 | 见 02-data-arch.html |
| 架构图包含功能模块关联关系架构 | 见 03-module-relations.html |
| 针对「环路」执行链路完整梳理 | 见本文档第 5 节 + 04-loop-execution-flow.html |
| 针对「事项任务」执行链路完整梳理 | 见本文档第 6 节 + 05-todo-execution-flow.html |
| 画成流程图式的框图表现形式 | 5 张图均为 SVG 框图，含泳道/判断/数据/起止等流程图元素 |
