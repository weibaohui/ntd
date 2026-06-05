# 项目目录

> **位置**：设置 → 项目目录
> **前端**：`frontend/src/components/settings/ProjectDirectoriesPanel.tsx`
> **后端**：`backend/src/handlers/project_directory.rs`

「项目目录」是 ntd **Todo workspace 的白名单**。当 Todo 跑起来时，ntd 会 cd 到这个目录执行命令。如果路径不在白名单里，CLI 工具可能拒绝运行（出于安全考虑）。

---

## 1. 数据模型

| 字段 | 含义 |
|------|------|
| `id` | 内部 ID |
| `path` | 绝对路径（如 `/Users/me/projects/myapp`） |
| `alias` | 备注名（如 `myapp`），方便选择 |
| `created_at` | 添加时间 |

---

## 2. 操作

| 操作 | 入口 |
|------|------|
| 新增 | 右上「+ 新增项目目录」 |
| 编辑 | 列表点「编辑」 |
| 删除 | 列表点「删除」 |

### 2.1 新增

1. 点「+ 新增」
2. 填 path（绝对路径）+ alias
3. 保存 → 后端检查目录是否存在（不存在报错）

### 2.2 路径检查

- 必须是**绝对路径**（以 `/` 开头）
- 目录必须**存在**
- 不允许符号链接绕过（防路径穿越）

---

## 3. Todo 工作流

### 3.1 TodoDrawer 选择 workspace

新建/编辑 Todo 时，「**工作目录**」下拉框：

- 选项 = 项目目录白名单全部
- 选完存到 `todo.workspace` 字段

### 3.2 执行时

- 执行器拿到 Todo 后，ntd 后端 cd 到 `todo.workspace`
- 如果 workspace 已被删/重命名 → 执行器报错
- 解决：在项目目录 Tab 更新路径，或新建同名路径

---

## 4. 与 Git Worktree

Todo 还有一个 `worktree_enabled` 字段：
- 开启后，ntd 在 `todo.workspace` 下创建 git worktree（独立分支）跑任务
- 适合「每个 Todo 一个分支」的并行开发
- 详见 [todo-lifecycle.md](../features/todo-lifecycle.md)

---

## 5. 故障排查

### 5.1 新增路径报「目录不存在」

- 路径拼错了
- 目录被删了
- 没权限访问（`ls` 试试）

### 5.2 Todo 选了工作目录但跑不起来

- 看后端日志，搜 `workspace` 关键字
- 检查目录权限：`chmod 755 /path/to/dir`
- 符号链接 / 挂载点可能不被识别

### 5.3 跨机器同步工作目录

- 项目目录**不会**随云端同步走
- 同步是 title + prompt 级别的，路径保留本机
- 在新机器上要重新配置项目目录

---

## 6. 相关 API

`project_directory::routes()` 挂载在 `/api/project-directories` 下，标准 CRUD。
