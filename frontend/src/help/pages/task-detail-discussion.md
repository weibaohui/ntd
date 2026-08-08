# 讨论区（论坛跟帖 + @触发执行）

## 功能位置

任务（详情） → Tab 4「讨论」（060 新增）。论坛式楼层列表 + 底部编辑器（`DiscussionComposer`），正文支持 Markdown；人帖中 `@专家名` / `@执行器名` 会触发对应智能体执行，执行结论以智能体帖自动回楼。

## 数据流图（前端 → 后端）

```mermaid
flowchart LR
  U([用户发表跟帖]) --> composer["DiscussionComposer<br/>Markdown 输入 + @ 提及"]
  composer -->|"POST /api/v1/workspaces/{ws}/tasks/{id}/posts<br/>{content, mentions, parent_post_id?}"| create["handlers::task_posts::create_post"]
  create -->|"人帖 kind=human 直接入库"| posts_tbl[(task_posts 表)]
  create -->|"含 @专家/@执行器"| spawn["触发智能体执行<br/>写 kind=agent 占位帖 status=running"]
  spawn --> records[(execution_records 表)]
  spawn -->|"执行完成回写"| agent_post["智能体帖<br/>status=success/failed<br/>content=执行结论"]
  agent_post --> posts_tbl
  list["DiscussionTab / useDiscussionPosts"] -->|"GET .../posts?page=N<br/>主楼层分页 + 楼中楼 replies"| posts_tbl
  posts_tbl -->|"主楼层 id ASC<br/>replies 深度 ≤1"| list
  poll["帖子页轮询"] -->|"GET .../posts/{pid}<br/>占位帖状态刷新"| posts_tbl
```

## 调用关系链路图

```mermaid
flowchart TD
  tab["TaskDetailPanel Tabs<br/>key=discussion"] --> dt["DiscussionTab<br/>(taskId, workspaceId)"]
  dt --> hook["useDiscussionPosts<br/>分页加载 / 发帖 / 轮询"]
  hook --> list_api["bundledApi.listTaskPosts<br/>GET .../tasks/{id}/posts"]
  hook --> create_api["bundledApi.createTaskPost<br/>POST .../tasks/{id}/posts"]
  dt --> cards["PostCard 楼层渲染<br/>mentions 徽标 JSON.parse<br/>智能体帖状态 Tag"]
  create_api -->|"返回人帖 + 占位智能体帖"| refresh["列表刷新<br/>轮询占位帖直至 success/failed"]
  cards -->|"点执行明细跳转"| post_page["帖子页<br/>?from=task&taskId=<id>"]
  post_page -->|"返回按钮"| back["/#/tasks/<id>?tab=discussion<br/>恢复讨论 Tab 选中态"]
```

## 数据结构图

```mermaid
classDiagram
  class TaskPost {
    +id: number
    +task_id: number
    +parent_post_id: number|null «楼中楼≤1 层»
    +kind: human | agent
    +author_name: string
    +executor: string|null
    +expert_name: string|null
    +content: string «Markdown»
    +mentions: string «TaskMention[] JSON»
    +status: sent | running | success | failed
    +source_execution_id: number|null
    +replies?: TaskPost[] «仅主楼层列表返回»
  }
  class TaskMention {
    +type: expert | executor
    +name: string «规范名»
    +display: string «展示名»
  }
  TaskPost --> TaskMention : mentions 列 JSON 序列化
```

## 数据变更图

```mermaid
stateDiagram-v2
  [*] --> 人帖sent: 发帖成功（kind=human，status 恒 sent）
  [*] --> 占位running: 含 @提及 → 触发执行 + kind=agent 占位帖
  占位running --> 智能体success: 执行完成，content 写入结论
  占位running --> 智能体failed: 执行失败，content 写入错误说明
  note right of 占位running: 前端轮询 GET .../posts/{pid}\n直至终态
  note right of 人帖sent: 纯评论不触发执行\n无后续状态流转
```

## 开发指导

- **前端入口**：`frontend/src/components/tasks/discussion/`——`DiscussionTab`（Tab 容器）、`useDiscussionPosts`（分页/发帖/轮询 hook）、`PostCard`（楼层 + mentions 徽标）、`DiscussionComposer`（编辑器 + @ 提及）；类型在 `frontend/src/types/task.ts`（`TaskPost` / `TaskMention`）。
- **后端入口**：`backend/src/handlers/task_posts.rs`——`GET .../posts`（主楼层分页 + replies 组装）、`GET .../posts/{pid}`（单帖，轮询用）、`POST .../posts`（人帖入库 + @ 触发执行 + 占位帖）；mentions 以 JSON 字符串存 `task_posts.mentions` 列。
- **注意**：楼中楼深度 ≤1（仅允许回复主楼层）；人帖 `status` 恒 `sent`，只有智能体帖有 running/success/failed 流转；从讨论 Tab 跳入帖子页时 URL 带 `?from=task&taskId=<id>`，返回据此回到 `#/tasks/<id>?tab=discussion`（TAB_KEYS 白名单含 discussion）。
- **扩展**：新增提及类型时在 `TaskMention.type` 加枚举值、后端 `create_post` 的触发分支加对应执行路径；智能体帖的 `source_execution_id` 已支持跳转执行明细。
