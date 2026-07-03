# Loop Blackboard CLI 设计

> 作者: Claude · 日期: 2026-07-03 · 分支: `feat/loop-blackboard-query`

## 1. 背景

ntd 的 Loop Studio 提供「多步骤循环执行」能力：用户预先定义一组有序的 step（每个 step 是一个 todo 模板），触发后引擎按顺序跑完所有 step。每一步的输出会作为下一步 prompt 的输入，并通过 `{{blackboard}}`、`{{last_output}}`、`{{last_conclusion}}` 等占位符注入。

**Blackboard（黑板）**就是这一机制的对外视图：让用户能**一眼看清 loop 当前跑到哪一步、各步的执行结论是什么**。它不是一个独立的表或列，而是从 `loop_step_executions` 表按 `sequence_index ASC` 排序后渲染出来的虚拟视图——每行 `conclusion` 字段就是该步骤写入黑板的内容。

前端已有 `BlackboardDrawer.tsx` 渲染同样的视图，但 CLI 端一直缺失。本设计为 `ntd` CLI 新增 `ntd loop execution blackboard <execution_id>` 命令，把这个能力带到命令行。

## 2. 命令语法

```
ntd loop execution blackboard <execution_id>            # 默认 JSON（AI/脚本友好）
ntd loop execution blackboard <execution_id> --human    # 人类可读黑板视图
```

| 选项 | 说明 |
|------|------|
| `<execution_id>` | 必填，loop_execution 主键 |
| `--human` | 输出人类可读黑板视图（默认是 JSON） |

### 2.1 为什么默认 JSON

CLI 的主要消费者是 **AI（Claude Code 等执行器）和 shell 脚本**，人类有更好的 UI（前端 BlackboardDrawer）。所以默认走 JSON，管道友好；要看图的人显式加 `--human`。

### 为什么放在 `execution` 子命令下

- 与 `ntd loop execution get <execution_id>` 同级，语义一致
- 「blackboard 是 execution 的一种视图」心智模型
- 未来 `approve`、`logs` 等命令可一并归位

## 3. 输出格式

### 3.1 默认（JSON）

直接打印 `GET /api/loop-executions/{eid}` 的响应体（`LoopExecutionDetail` 的全部字段），便于 AI/脚本消费：

```json
{
  "id": 1105,
  "loop_id": 1,
  "loop_name": "笑话工厂",
  "status": "success",
  "total_steps": 1,
  "completed_steps": 1,
  "failed_steps": 0,
  "started_at": "2026-07-03T00:10:23.855Z",
  "finished_at": "2026-07-03T00:10:29.988Z",
  "step_executions": [
    {
      "id": 1106,
      "sequence_index": 1,
      "step_id": 1,
      "step_name": "讲个笑话",
      "status": "success",
      "execution_record_id": 1137,
      "conclusion": "为什么程序员总是分不清万圣节和圣诞节？因为 Oct 31 等于 Dec 25！",
      "rating": null,
      "input_tokens": 13003,
      "output_tokens": 628
    }
  ],
  "token_summary": {
    "total_input_tokens": 13003,
    "total_output_tokens": 628,
    "total_cache_read_input_tokens": 0,
    "total_cache_creation_input_tokens": 0,
    "total_cost_usd": 0.0
  }
}
```

### 3.2 `--human`（人类视图）

```
═══ Loop Execution #42 ────────────────────────────────────
循环: 每日代码 review
触发: cron @ 0 9 * * *
状态: ✅ success · 完成 3/3 步
开始: 2026-07-03 09:00:00 · 结束: 09:45:32

  #1 ✅ success          编写 CRUD 代码             评分 85
     exec: #1024
     完成了用户登录功能的 CRUD 代码

  #2 ✅ success          补充单元测试               评分 90
     exec: #1025
     新增 12 个测试用例，覆盖率提升到 87%

  #3 ⏭️ skipped          更新 README                 评分 -
     exec: -
     (无结论)
     原因: 步骤已跳过（依赖 #2 失败）

═══ 3 步 / Token: 输入 12k 输出 5k ════════════════════════
```

### 3.3 JSON 模式

见 3.1 默认输出（已切换为 JSON）。

### 3.3 字段映射

| 输出 | 数据来源 |
|------|---------|
| 循环名 | `LoopExecutionDetail.loop_name` |
| 触发信息 | `execution.trigger_meta`（cron / manual / webhook / feishu） |
| 状态图标 | `execution.status` → emoji |
| 完成数 | `execution.completed_steps / total_steps` |
| 开始/结束时间 | `execution.started_at / finished_at` |
| 每步状态图标 | `step.status` → emoji |
| 每步名称 | `step.step_name`（异常处理步骤显示「异常处理」） |
| 每步评分 | `step.rating`（无评分显示 `-`） |
| 每步结论 | `step.conclusion`（无显示 `(无结论)`） |
| 每步 record id | `step.execution_record_id`（无显示 `-`） |
| 失败原因 | `step.error_message` |
| 待审批评论 | `step.approval_comment` |
| Token 汇总 | `LoopExecutionDetail.token_summary` |

### 3.4 状态图标约定

| status | 图标 |
|--------|------|
| `success` | ✅ |
| `failed` | ❌ |
| `running` | ⏳ |
| `pending` | ⏸ |
| `pending_approval` | 🤔 |
| `skipped` | ⏭️ |
| 其他 | ❔ |

约定与前端 `BlackboardDrawer.tsx` 保持一致。

## 4. 边界处理

| 场景 | 行为 |
|------|------|
| execution 不存在 | `API error 404: ...`，由 `print_response` 标准错误流程处理 |
| execution 存在但 step_executions 为空 | 打印「黑板为空（loop 尚未执行任何步骤）」，不报错 |
| step 有 `error_message` 但 `conclusion` 为空 | 用 `error_message` 替代结论显示，并标红前缀 `失败:` |
| step `status=pending_approval` | 显示 `🤔 待审批` 标题，附 `approval_comment` |
| `execution_record_id = None`（pending / skipped / 异常） | 显示 `exec: -` |

## 5. 设计取舍

### 5.1 为什么不加新的 API 端点

`GET /api/loop-executions/{eid}` 已经返回完整的 `LoopExecutionDetail`，包含按 `sequence_index` 排序的 `step_executions[]`。新增端点只是重复造轮子、增加维护成本，并可能与 DTO 漂移。CLI 端渲染逻辑放在客户端，避免服务端为不同 client（CLI / Feishu / 前端）重复实现 3 遍。

### 5.2 为什么默认输出 JSON 而不是人类可读

CLI 的主要消费者是 **AI / 脚本**：Claude Code、其他执行器、cron 包装、监控告警等。人类调试场景存在但不是主诉求，且人类有更好的 UI（前端 BlackboardDrawer）。所以默认走 JSON，加 `--human` 走专用视图。

历史版本曾把人类视图作为默认，但实际使用中 AI/脚本管道消费 JSON 是绝对主流（grep、jq、CI），人类随时可以 `--human` 切。

### 5.3 为什么不在 backend 新增黑板表

现有 `loop_step_executions.conclusion` 列已经能 cover 需求，新增表是冗余（违反 YAGNI）。如果未来需要按 `key` 索引等能力，再考虑结构化改造。

### 5.4 渲染纯文本而不是彩色

终端 ANSI 颜色在脚本管道、`less`、`>> file` 等场景下会有干扰。当前阶段保持纯文本最简单；后续若用户反馈需要高亮，再加 `--color` 开关。

## 6. 实现方案

### 6.1 改动清单

| 文件 | 改动 | 行数 |
|------|------|------|
| `backend/src/cli/commands.rs` | `LoopExecutionAction` 加 `Blackboard { execution_id, json }` 变体 | +10 |
| `backend/src/cli/commands.rs` | `handle_loop` 加 match arm | +18 |
| `backend/src/cli/commands.rs` | 新增 `render_blackboard(data: Option<&Value>)` 渲染函数（handle_loop 直传，不强制预先过滤） | ~70 |
| `backend/src/cli/commands.rs` | 新增 `status_icon(status: &str) -> &'static str` 帮助函数 | ~12 |
| `backend/src/cli/commands.rs` | 单测 6 个：normal / no_record_id / empty / failed / pending_approval / status_icon | ~80 |
| `docs/ntd-cli.md` | 命令列表加一条 + 示例输出 | +25 |
| `docs/loop-blackboard-cli.md` | 本文档 | ~180 |

**总计 ~395 行，1 个源文件 + 2 个文档，零后端业务代码、零依赖增量。**

### 6.2 测试策略

| 测试 | 输入 | 断言 |
|------|------|------|
| `test_status_icon_*` | 各 status 字符串 | 返回正确 emoji |
| `test_render_blackboard_normal` | 3 step 全 success | 输出含 3 行 `exec: #N`、3 个 ✅、按 sequence_index 升序 |
| `test_render_blackboard_no_record_id` | step 的 `execution_record_id=null` | `exec: -` 出现 |
| `test_render_blackboard_empty` | `step_executions=[]` | 输出「黑板为空」 |
| `test_render_blackboard_failed` | step 状态 `failed`，有 `error_message` | 显示 ❌，结论行替换为 `失败: <msg>` |
| `test_render_blackboard_pending_approval` | step 状态 `pending_approval` | 显示 🤔，包含 `approval_comment` |

### 6.3 真机验证

启动开发 daemon，构造一个已完成的 loop execution，运行：

```bash
ntd loop execution blackboard <eid>
ntd loop execution blackboard <eid> --json
ntd loop execution blackboard 99999   # 不存在的 ID
```

肉眼比对输出与 `BlackboardDrawer.tsx` 的前端渲染。

## 7. 影响范围

- **后端业务逻辑**：零改动
- **数据库 schema**：零改动
- **前端**：零改动
- **API**：零新增端点
- **CLI**：新增 1 个子命令
- **文档**：2 个 md 文件

## 8. 后续可演进方向

1. 加 `--color` 开关输出 ANSI 高亮（failed 红、success 绿）
2. 加 `--follow` 模式轮询（每 5s 刷新，用于调试长 loop）
3. 加 `--step <name>` 过滤只显示某一步
4. 加 `--from <seq>` / `--to <seq>` 切片
5. 性能优化：loop execution 巨大（>1000 step）时分页