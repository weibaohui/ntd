# 黑板（Blackboard）设计方案

## 1. 核心概念

黑板是一个**简化版 LLM-Wiki**：每个工作空间维护一个 `blackboard.md`，由 LLM 自动维护，前端直接渲染展示。

### 一进一出

```
输入：任务执行结论（Finished.result）
  ↓
中间：LLM 读取当前黑板 + 新结论 → 生成更新后的黑板
  ↓
输出：blackboard.md（前端页面展示）
```

### 关键设计决策

| 维度 | 方案 |
|------|------|
| 知识库形态 | 简化版 single-file Wiki，只有 `blackboard.md` |
| 存储方式 | 数据库存储（非文件系统） |
| 工作空间隔离 | 每个 workspace 独立黑板 |
| 前端展示 | 直接渲染 `blackboard.md` |
| 用户编辑 | **Phase 1 不做**，只读展示 |
| 版本历史 | **不做**，只保留当前黑板 |
| 触发方式 | 自动（任务完成）+ 手动 |
| 更新失败 | 静默失败，记录日志 |
| 菜单名称 | 黑板，放在环路菜单下面 |
| 来源引用 | 带来源 ID，可跳转到 Todo 详情 |
| 新工作空间 | 空内容 |
| LLM 模型 | 系统配置的 executor 模型 |

---

## 2. 数据模型

### 2.1 blackboards 表（当前黑板）

```sql
CREATE TABLE IF NOT EXISTS blackboards (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    workspace_id INTEGER NOT NULL UNIQUE,
    content TEXT NOT NULL DEFAULT '',
    updated_at TEXT,
    created_at TEXT,
    FOREIGN KEY (workspace_id) REFERENCES project_directories(id) ON DELETE CASCADE
);
```

| 字段 | 说明 |
|------|------|
| `id` | 自增主键 |
| `workspace_id` | 关联 project_directories.id，唯一 |
| `content` | 当前黑板 Markdown 内容 |
| `updated_at` | 最后更新时间 |
| `created_at` | 创建时间 |

### 2.2 Entity 定义

```rust
// backend/src/db/entity/blackboards.rs
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "blackboards")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub workspace_id: i64,
    #[sea_orm(column_type = "Text")]
    pub content: String,
    pub updated_at: Option<String>,
    pub created_at: Option<String>,
}
```

---

## 3. blackboard.md 内容结构

```markdown
# 工作空间进展

## 已确认
- 结论 A（来源: [todo_123](#/items?id=123)）
- 结论 B（来源: [todo_124](#/items?id=124)）

## 新发现
- 来自 todo_125：发现了 XX

## 待解决问题
- 问题 1
- 问题 2

## 矛盾/风险
- 结论 A 与 结论 C 存在矛盾

## 下一步建议
- 建议执行某个任务验证问题 1
```

### 来源链接格式

LLM 输出时使用特殊标记，前端渲染时转换为可点击链接：

```markdown
来源: [todo_123](ntd://todo/123)
```

前端渲染器识别 `ntd://todo/{id}` 协议，转换为跳转到 Todo 详情的链接。

---

## 4. API 设计

### 4.1 获取当前黑板

```
GET /api/workspaces/{workspace_id}/blackboard
```

响应：
```json
{
  "id": 1,
  "workspace_id": 1,
  "content": "# 工作空间进展\n\n## 已确认\n...",
  "updated_at": "2026-07-03T10:00:00Z"
}
```

### 4.2 手动触发更新

```
POST /api/workspaces/{workspace_id}/blackboard/refresh
```

响应：
```json
{
  "success": true,
  "message": "黑板更新已触发"
}
```

---

## 5. LLM 更新流程

### 5.1 触发时机

**自动触发**：任务执行完成时（`ExecEvent::Finished`）

> 注：手动刷新功能已移除。用户可通过刷新页面（刷新按钮 → `window.location.reload()`）重新拉取黑板内容，无需 LLM 参与。

### 5.2 更新逻辑

```rust
async fn update_blackboard(
    db: &Database,
    workspace_id: i64,
    new_conclusion: &str,  // Finished.result
    todo_id: i64,
    todo_title: &str,
) -> Result<(), Error> {
    // 1. 读取当前黑板内容
    let current = db.get_blackboard(workspace_id).await?.unwrap_or_default();
    
    // 2. 调用 LLM 生成新内容
    let prompt = build_blackboard_prompt(&current.content, new_conclusion, todo_id, todo_title);
    let new_content = call_llm(&prompt).await?;
    
    // 3. 更新当前黑板
    db.update_blackboard(workspace_id, &new_content).await?;
    
    Ok(())
}
```

### 5.3 Prompt 设计

```
你是一个工作空间知识库的维护者。你的任务是维护一个 Markdown 格式的"黑板"，
记录工作空间中所有任务执行的结论和当前进展。

当前黑板内容：
```
{current_blackboard}
```

新任务结论：
- 任务 ID: {todo_id}
- 任务标题: {todo_title}
- 执行结论: {conclusion}

请更新黑板内容，要求：
1. 将新结论整合到黑板中
2. 保持以下结构：
   - # 工作空间进展
   - ## 已确认
   - ## 新发现
   - ## 待解决问题
   - ## 矛盾/风险
   - ## 下一步建议
3. 每条结论标注来源，格式：(来源: [todo_{id}](ntd://todo/{id}))
4. 如果新结论与已有结论矛盾，在"矛盾/风险"中标注
5. 如果新结论提出了未解决的问题，在"待解决问题"中列出
6. 更新"下一步建议"
7. 保持 Markdown 格式，不要添加 HTML
8. 如果黑板为空，根据新结论创建初始结构

只输出更新后的黑板内容，不要输出任何解释。
```

### 5.4 LLM 调用方式（复用现有 Action/Todo 执行机制）

不复用 HTTP 直接调用，而是复用现有的 **Action/Todo 执行机制**：

1. **固定 Todo 模板**：`action_type="blackboard"`, `action_key="update"`
2. **动态 Prompt**：通过 `params` 传入当前黑板内容 + 新结论，替换占位符
3. **复用执行器**：走 `run_todo_execution` → 子进程执行 → 解析结果
4. **同步等待**：Service 层订阅 broadcast channel，等待 `Finished` 事件，提取 `result`

**复用的现有代码**：

| 复用点 | 来源 |
|--------|------|
| `find_or_create_todo` | `handlers/action.rs` |
| `replace_placeholders` | `handlers/action.rs` |
| `run_todo_execution` | `executor_service/mod.rs` |
| `ExecEvent::Finished` | `executor_service/events.rs` |
| broadcast channel | 现有的 `tx` |

**黑板更新 Service 核心逻辑**：

```rust
pub async fn update_blackboard(
    db: Arc<Database>,
    executor_registry: Arc<ExecutorRegistry>,
    tx: broadcast::Sender<ExecEvent>,
    task_manager: Arc<TaskManager>,
    config: Arc<std::sync::RwLock<Config>>,
    workspace_id: i64,
    conclusion: &str,
    source_todo_id: i64,
    source_todo_title: &str,
) -> Result<(), AppError> {
    // 1. 读取当前黑板内容
    let current = db.get_blackboard(workspace_id).await?;
    let current_content = current.map(|b| b.content).unwrap_or_default();
    
    // 2. 查找或创建 blackboard update todo
    let (todo_id, _) = find_or_create_blackboard_todo(&db, workspace_id).await?;
    
    // 3. 构造 prompt（复用 Action 的占位符替换机制）
    let prompt = build_blackboard_prompt();
    let mut params = HashMap::new();
    params.insert("current".to_string(), current_content);
    params.insert("conclusion".to_string(), conclusion.to_string());
    params.insert("todo_id".to_string(), source_todo_id.to_string());
    params.insert("todo_title".to_string(), source_todo_title.to_string());
    let message = replace_placeholders(&prompt, &params);
    
    // 4. 启动执行（复用 run_todo_execution）
    let result = crate::executor_service::run_todo_execution(
        RunTodoExecutionRequest {
            db: db.clone(),
            executor_registry,
            tx: tx.clone(),
            task_manager,
            config,
            todo_id,
            message,
            req_executor: None,
            trigger_type: "blackboard".to_string(),
            params: None,
            resume_session_id: None,
            resume_message: None,
            source_todo_id: Some(source_todo_id),
            source_todo_title: Some(source_todo_title.to_string()),
            feishu_bot_id: None,
            feishu_receive_id: None,
            loop_step_execution_id: None,
            step_id: None,
            workspace_path: None,
            workspace_id: Some(workspace_id),
        },
    ).await;
    
    let record_id = result.record_id.ok_or_else(|| {
        AppError::Internal("黑板更新任务启动失败".to_string())
    })?;
    
    // 5. 订阅 broadcast channel 等待 Finished 事件
    let mut rx = tx.subscribe();
    while let Ok(event) = rx.recv().await {
        if let ExecEvent::Finished { record_id: finished_rid, result: Some(new_content), .. } = event {
            if finished_rid == record_id {
                // 6. 更新黑板
                db.update_blackboard(workspace_id, &new_content).await?;
                return Ok(());
            }
        }
    }
    
    Ok(())
}
```

---

## 6. 前端设计

### 6.1 路由

```
/?view=blackboard
```

### 6.2 菜单

在 LeftRail 的"工作区"分组中，"环路"下方添加：

```typescript
{ key: 'blackboard', label: '黑板', icon: <FormOutlined />, ariaLabel: '黑板' }
```

图标使用 `FormOutlined`（Ant Design 的表单/文档图标，接近黑板概念）。

### 6.3 页面布局

```
┌─────────────────────────────────────────────┐
│ 黑板                              [设置按钮]  │
├─────────────────────────────────────────────┤
│                                             │
│  # 工作空间进展                              │
│                                             │
│  ## 已确认                                   │
│  - ...                                      │
│                                             │
│  ## 新发现                                   │
│  - ...                                      │
│                                             │
│  ...                                        │
│                                             │
└─────────────────────────────────────────────┘
```

### 6.4 Markdown 渲染

使用 `@ant-design/x-markdown` 渲染 blackboard.md 内容。

自定义链接处理：识别 `ntd://todo/{id}` 协议，点击后跳转到 Todo 详情页。

```typescript
const components = {
  a: (props: any) => {
    const href = props.href as string;
    if (href.startsWith('ntd://todo/')) {
      const todoId = href.replace('ntd://todo/', '');
      return <a onClick={() => navigateToTodo(Number(todoId))}>{props.children}</a>;
    }
    return <a {...props} />;
  },
};
```

---

## 7. 与现有系统的集成

### 7.1 Finished 事件 Hook

在 `executor_service/completion.rs` 的 `emit_completion_events` 或 `finalize_normal_completion` 中，添加黑板更新调用：

```rust
// 在 emit_completion_events 之后
async fn maybe_update_blackboard(
    db: &Arc<Database>,
    executor_registry: &Arc<ExecutorRegistry>,
    tx: &broadcast::Sender<ExecEvent>,
    task_manager: &Arc<TaskManager>,
    config: &Arc<std::sync::RwLock<crate::config::Config>>,
    todo_id: i64,
    workspace_id: Option<i64>,
    result: &str,
    todo_title: &str,
    todo_action_type: Option<&str>,
) {
    let Some(ws_id) = workspace_id else { return };
    
    // 跳过 blackboard update todo 自身，避免无限循环
    if todo_action_type == Some("blackboard") {
        return;
    }
    
    let result = result.to_string();
    let db = db.clone();
    let executor_registry = executor_registry.clone();
    let tx = tx.clone();
    let task_manager = task_manager.clone();
    let config = config.clone();
    let todo_title = todo_title.to_string();
    
    // 异步执行，不阻塞事件流
    tokio::spawn(async move {
        if let Err(e) = crate::services::blackboard::update_blackboard(
            db, executor_registry, tx, task_manager, config,
            ws_id, &result, todo_id, &todo_title
        ).await {
            tracing::warn!("黑板更新失败: {}", e);
        }
    });
}
```

### 7.2 更新失败处理

- LLM 调用失败：静默失败，记录 warn 日志
- 数据库写入失败：静默失败，记录 error 日志
- 不影响任务执行的正常流程

---

## 8. 开发路线图

| 阶段 | 任务 | 说明 |
|------|------|------|
| **Phase 1** | 数据库层 | Entity + Migration + DB 方法 |
| **Phase 2** | 后端 API | Handler + Service + 路由注册 |
| **Phase 3** | LLM 更新逻辑 | Prompt + 调用 + Finished 事件 Hook |
| **Phase 4** | 前端页面 | 菜单 + 页面 + API 调用 + Markdown 渲染 |
| **Phase 5** | 手动刷新 | ~~刷新按钮~~ → 页面 reload（无需 LLM） |

---

## 9. 边界情况

### 9.1 首次使用
- 新工作空间创建时，黑板为空字符串
- 第一次任务完成后，LLM 根据单条结论生成初始黑板

### 9.2 黑板内容过长
- Phase 1 不做限制
- 未来可考虑：LLM 提示中要求精简，或分块处理

### 9.3 并发更新
- 使用数据库行锁或乐观锁
- Phase 1 先不做特殊处理，依赖 SQLite 的串行写入

### 9.4 LLM 返回格式错误
- 如果 LLM 没有按预期格式返回，直接保存原始内容
- 前端仍然能渲染 Markdown

### 9.5 任务结论为空
- 如果 `Finished.result` 为空，跳过黑板更新

---

## 10. 未来扩展（Phase 2+）

- **用户编辑**：允许用户直接修改黑板内容，LLM 后续更新时尊重用户修改
- **增量更新**：不发送完整黑板，只发送变更摘要，降低成本
- **定时刷新**：配置自动刷新间隔
- **主题颜色**：不同主题使用不同颜色标签
- **搜索功能**：搜索黑板内容
- **智能体建议**：基于黑板内容自动生成 Todo 建议
- **Loop 结论**：Loop 执行完成后也更新黑板
