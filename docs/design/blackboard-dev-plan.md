# 黑板（Blackboard）开发计划

> 基于 [blackboard-design.md](blackboard-design.md) 的实现任务拆分。

---

## 开发阶段总览

| 阶段 | 任务数 | 预估工时 | 核心产出 |
|------|--------|----------|----------|
| **Phase 1** 数据库层 | 3 个 | 2h | blackboards 表 + DB 方法 |
| **Phase 2** 后端 API | 3 个 | 3h | Handler + Service + 路由注册 + 验证 |
| **Phase 3** LLM 更新 | 2 个 | 3h | Prompt + LLM 调用 + Finished 事件 Hook |
| **Phase 4** 前端页面 | 4 个 | 4h | 菜单 + 页面 + API + Markdown 渲染 |
| **Phase 5** 手动刷新 | 1 个 | 1h | 刷新按钮 + 状态提示 |

**总计：13 个任务，约 13 小时**

---

## Phase 1：数据库层

### 任务 1.1：创建 Entity 定义

**工作内容：**
- 创建 `backend/src/db/entity/blackboards.rs`
- 在 `backend/src/db/entity/mod.rs` 中注册新实体
- 在 `backend/src/db/entity/prelude.rs` 中导出

**产出物：**
- `backend/src/db/entity/blackboards.rs`（SeaORM Entity）

**验证方法：**
```bash
cd backend && cargo check
# 预期：编译通过，无错误
```

---

### 任务 1.2：创建数据库迁移

**工作内容：**
- 创建 `backend/src/db/migration/v47.rs`
- 在 `backend/src/db/migration/mod.rs` 中注册 V47
- 包含 blackboards 表的 CREATE TABLE

**SQL 内容：**
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

**产出物：**
- `backend/src/db/migration/v47.rs`
- `backend/src/db/migration/mod.rs` 更新

**验证方法：**
```bash
cd backend && cargo test db::migration::v47_tests -- --nocapture
# 预期：测试通过，表结构正确
```

---

### 任务 1.3：实现 DB 层方法

**工作内容：**
- 在 `backend/src/db/mod.rs` 或新建 `backend/src/db/blackboard.rs` 中实现：
  - `get_blackboard(workspace_id: i64) -> Result<Option<Blackboard>, DbErr>`
  - `create_blackboard(workspace_id: i64) -> Result<Blackboard, DbErr>`
  - `update_blackboard(workspace_id: i64, content: &str) -> Result<(), DbErr>`


**产出物：**
- `backend/src/db/blackboard.rs`
- `backend/src/db/mod.rs` 中注册模块

**验证方法：**
```bash
cd backend && cargo test db::blackboard_tests -- --nocapture
# 预期：测试通过，CRUD 操作正确
```

---

## Phase 2：后端 API

### 任务 2.1：创建 Blackboard Service

**工作内容：**
- 创建 `backend/src/services/blackboard.rs`
- 实现 `find_or_create_blackboard_todo`：查找或创建 `action_type="blackboard"`, `action_key="update"` 的 Todo 模板
- 实现 `update_blackboard` 方法（复用 Action 机制，见 Phase 3 详细逻辑）
- 实现 `refresh_blackboard` 方法（手动刷新：重新执行 blackboard update todo）

**核心逻辑（简化版，详见 Phase 3）：**
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
    // 1. 读取当前黑板
    let current = db.get_blackboard(workspace_id).await?;
    let current_content = current.map(|b| b.content).unwrap_or_default();
    
    // 2. 查找/创建 blackboard update todo（action_type="blackboard", action_key="update"）
    let (todo_id, _) = find_or_create_blackboard_todo(&db, workspace_id).await?;
    
    // 3. 构造 message（占位符替换）
    let message = build_message(&current_content, conclusion, source_todo_id, source_todo_title);
    
    // 4. 启动执行（复用 run_todo_execution）
    let result = crate::executor_service::run_todo_execution(...).await;
    
    // 5. 等待 Finished 事件并更新黑板（详见 Phase 3）
    ...
}
```

**产出物：**
- `backend/src/services/blackboard.rs`
- `backend/src/services/mod.rs` 中注册模块

**验证方法：**
```bash
cd backend && cargo test services::blackboard_tests -- --nocapture
# 预期：测试通过，更新逻辑正确
```

---

### 任务 2.2：创建 Blackboard Handler

**工作内容：**
- 创建 `backend/src/handlers/blackboard.rs`
- 实现 2 个 API 端点：
  - `GET /api/workspaces/{workspace_id}/blackboard`
  - `POST /api/workspaces/{workspace_id}/blackboard/refresh`

**产出物：**
- `backend/src/handlers/blackboard.rs`

**验证方法：**
```bash
cd backend && cargo check
# 预期：编译通过
```

---

### 任务 2.3：注册路由

**工作内容：**
- 在 `backend/src/handlers/mod.rs` 中：
  - `mod blackboard;`
  - 新增 `blackboard_routes()` 函数
  - 在 `mount_domain_routes()` 中 `.merge(blackboard_routes())`
- 在 `backend/src/db/mod.rs` 中注册 `blackboard` 模块

**产出物：**
- `backend/src/handlers/mod.rs` 更新
- `backend/src/db/mod.rs` 更新

**验证方法：**
```bash
cd backend && cargo test handlers::mod_tests::each_domain_routes_function_returns_a_router -- --nocapture
# 预期：路由数量从 19 变为 20，测试通过
```

---

## Phase 3：LLM 更新

### 任务 3.1：实现黑板更新 Service（复用 Action 机制）

**工作内容：**
- 创建 `backend/src/services/blackboard.rs`
- 实现 `find_or_create_blackboard_todo`：查找或创建 `action_type="blackboard"`, `action_key="update"` 的 Todo 模板
- 实现 `build_blackboard_prompt`：构造包含占位符的 Prompt 模板
- 实现 `update_blackboard`：
  1. 读取当前黑板内容
  2. 查找/创建 blackboard update Todo
  3. 用 `replace_placeholders` 替换占位符（复用 `handlers/action.rs`）
  4. 调用 `run_todo_execution` 启动执行（复用 `executor_service/mod.rs`）
  5. 订阅 broadcast channel 等待 `Finished` 事件
  6. 提取 `result` 并更新 blackboards 表
- 实现 `refresh_blackboard`：手动刷新（重新执行 blackboard update todo）

**Prompt 模板：**
```rust
fn build_blackboard_prompt() -> String {
    r#"你是一个工作空间知识库的维护者。你的任务是维护一个 Markdown 格式的"黑板"，记录工作空间中所有任务执行的结论和当前进展。

当前黑板内容：
```
{{current}}
```

新任务结论：
- 任务 ID: {{todo_id}}
- 任务标题: {{todo_title}}
- 执行结论: {{conclusion}}

请更新黑板内容，要求：
1. 将新结论整合到黑板中
2. 保持以下结构：
   - # 工作空间进展
   - ## 已确认
   - ## 新发现
   - ## 待解决问题
   - ## 矛盾/风险
   - ## 下一步建议
3. 每条结论标注来源，格式：(来源: [todo_{{todo_id}}](ntd://todo/{{todo_id}}))
4. 如果新结论与已有结论矛盾，在"矛盾/风险"中标注
5. 如果新结论提出了未解决的问题，在"待解决问题"中列出
6. 更新"下一步建议"
7. 保持 Markdown 格式，不要添加 HTML
8. 如果黑板为空，根据新结论创建初始结构

只输出更新后的黑板内容，不要输出任何解释。"#.to_string()
}
```

**产出物：**
- `backend/src/services/blackboard.rs`
- `backend/src/services/mod.rs` 中注册模块

**验证方法：**
```bash
# 启动后端，执行一个 Todo
# 检查日志中是否有 "黑板更新" 相关日志
# 调用 API 查看黑板内容是否更新
curl http://localhost:3000/api/workspaces/1/blackboard
```

---

### 任务 3.2：Finished 事件 Hook

**工作内容：**
- 修改 `backend/src/executor_service/completion.rs`
- 在 `finalize_normal_completion` 中，发送 `Finished` 事件后，**仅当源任务不是 blackboard update todo 时**，异步触发黑板更新
- 在 `handle_cancellation_branch` 和 `handle_timeout_branch` 中跳过黑板更新（只有成功完成才更新）
- 黑板更新任务自身完成后也会产生 `Finished` 事件，但不应再次触发黑板更新（避免无限循环）

**Hook 位置：**
```rust
// 在 finalize_normal_completion 末尾
emit_completion_events(...);

// 新增：异步更新黑板（仅当源任务不是 blackboard update todo）
if success {
    if let Some(ws_id) = workspace_id {
        // 检查当前 todo 不是 blackboard update todo
        let is_blackboard_todo = todo_action_type.as_deref() == Some("blackboard");
        if !is_blackboard_todo {
            let db_clone = db.clone();
            let executor_registry_clone = executor_registry.clone();
            let tx_clone = tx.clone();
            let task_manager_clone = task_manager.clone();
            let config_clone = config.clone();
            let result_str_clone = result_str.clone();
            let todo_title_clone = todo_title.clone();
            tokio::spawn(async move {
                if let Err(e) = crate::services::blackboard::update_blackboard(
                    db_clone, executor_registry_clone, tx_clone, task_manager_clone, config_clone,
                    ws_id, &result_str_clone, todo_id, &todo_title_clone
                ).await {
                    tracing::warn!("黑板更新失败: {}", e);
                }
            });
        }
    }
}
```

**产出物：**
- `backend/src/executor_service/completion.rs` 更新

**验证方法：**
```bash
# 启动后端，执行一个 Todo
# 检查日志中是否有 "黑板更新" 相关日志
# 调用 API 查看黑板内容是否更新
curl http://localhost:3000/api/workspaces/1/blackboard
```

---

## Phase 4：前端页面

### 任务 4.1：添加菜单项

**工作内容：**
- 修改 `frontend/src/components/shell/LeftRail.tsx`
- 在 `LeftRailKey` 类型中添加 `'blackboard'`
- 在"工作区"分组的"环路"下方添加黑板菜单项
- 使用 `FormOutlined` 图标

**产出物：**
- `frontend/src/components/shell/LeftRail.tsx`

**验证方法：**
```bash
cd frontend && npm run build
# 预期：编译通过
# 刷新页面，左侧菜单应出现"黑板"选项
```

---

### 任务 4.2：添加路由

**工作内容：**
- 修改 `frontend/src/hooks/useViewState.ts`
- 在 `View` 类型中添加 `'blackboard'`
- 在 `ALL_VIEWS` 中添加 `'blackboard'`
- 在 `VIEW_TO_NAV_KEY` 中添加映射

**产出物：**
- `frontend/src/hooks/useViewState.ts`

**验证方法：**
```bash
cd frontend && npm run build
# 预期：编译通过
# 访问 /?view=blackboard，应能正常切换
```

---

### 任务 4.3：创建 BlackboardPage 组件

**工作内容：**
- 创建 `frontend/src/components/BlackboardPage.tsx`
- 页面布局：
  - 顶部标题栏："黑板" + 刷新按钮
  - 主体：Markdown 渲染区域（使用 `@ant-design/x-markdown`）
- 实现 `ntd://todo/{id}` 链接的自定义渲染
- 空状态：显示"暂无内容，任务执行后将自动更新"

**产出物：**
- `frontend/src/components/BlackboardPage.tsx`

**验证方法：**
```bash
cd frontend && npm run build
# 预期：编译通过
# 刷新页面，切换到黑板视图，应显示空状态或已有内容
```

---

### 任务 4.4：集成到 App.tsx

**工作内容：**
- 修改 `frontend/src/App.tsx`
- 导入 `BlackboardPage`
- 在 `activeView === 'blackboard'` 分支中渲染 `BlackboardPage`

**产出物：**
- `frontend/src/App.tsx`

**验证方法：**
```bash
cd frontend && npm run build
# 预期：编译通过
# 点击左侧"黑板"菜单，应显示 BlackboardPage
```

---

## Phase 5：手动刷新

### 任务 6.1：刷新按钮与状态提示

**工作内容：**
- 修改 `frontend/src/components/BlackboardPage.tsx`
- 刷新按钮点击后：
  - 显示"更新中..."状态
  - 调用 `POST /api/workspaces/{id}/blackboard/refresh`
  - 成功后刷新黑板内容
  - 失败显示错误提示
- 自动刷新：页面加载时自动获取最新内容

**产出物：**
- `frontend/src/components/BlackboardPage.tsx` 更新

**验证方法：**
```bash
# 点击刷新按钮
# 预期：显示 loading 状态，成功后黑板内容更新
```

---

## 验收标准

### 功能验收

| 检查项 | 验收标准 |
|--------|----------|
| 菜单显示 | 左侧导航栏出现"黑板"菜单项，位于"环路"下方 |
| 页面渲染 | 点击"黑板"进入黑板页面，显示 Markdown 内容 |
| 自动更新 | 任务执行完成后，黑板自动更新（延迟 5-30 秒） |
| 手动刷新 | 点击刷新按钮，触发 LLM 重新总结并更新 |

| 来源链接 | 黑板中的 `ntd://todo/{id}` 链接可点击跳转 |
| 空状态 | 新工作空间黑板为空，显示提示文案 |

### 技术验收

| 检查项 | 验收标准 |
|--------|----------|
| 编译通过 | `cd backend && cargo build` 成功 |
| 前端编译 | `cd frontend && npm run build` 成功 |
| 数据库迁移 | 新表自动创建，无报错 |
| API 可用 | 2 个 API 端点全部可调用 |
| 日志记录 | LLM 调用失败有 warn 日志 |
| 不影响主流程 | 黑板更新失败不影响任务执行 |

---

## 开发顺序建议

**按依赖关系排序：**

```
1.1 创建 Entity
  ↓
1.2 创建 Migration
  ↓
1.3 实现 DB 方法
  ↓
2.1 创建 Service
  ↓
2.2 创建 Handler
  ↓
2.3 注册路由
  ↓
3.1 实现 LLM 调用
  ↓
3.2 Finished 事件 Hook
  ↓
4.1 添加菜单项
  ↓
4.2 添加路由
  ↓
4.3 创建 BlackboardPage
  ↓
4.4 集成到 App.tsx
  ↓
5.1 刷新按钮与状态提示
```

**可并行开发：**
- Phase 1 + Phase 4.1/4.2（数据库和前端菜单无依赖）
- Phase 2 + Phase 4.3（可先用 mock 数据开发前端页面）
