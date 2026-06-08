# 项目目录

> **位置**：设置 →项目目录
> **前端**：`frontend/src/components/settings/ProjectDirectoriesPanel.tsx`
> **后端**：`backend/src/handlers/project_directory.rs`

「项目目录」是 ntd **Todo workspace 的白名单**。当 Todo跑起来时，ntd会 cd 到这个目录执行命令。如果路径不在白名单里，CLI工具可能拒绝运行（出于安全考虑）。

---

## 1.数据模型

`backend/src/db/project_directory.rs::ProjectDirectory`：

|字段 |含义 |
|------|------|
| `id` |内部 ID |
| `path` |绝对路径（如 `/Users/me/projects/myapp`） |
| `name` |项目名称（如 `myapp`），**必填** |
| `created_at` |添加时间 |
| `updated_at` |更新时间 |

>字段命名是 `name`（不是 `alias`），同时记录 `updated_at`。

---

## 2.操作

|操作 |入口 |
|------|------|
| 新增 |右上「+ 新增项目目录」 |
| 编辑 |列表点「编辑」 |
|删除 |列表点「删除」 |

### 2.1新增

1. 点「+ 新增」
2.填 path（绝对路径）+ name（**必填**）
3.保存 →后端检查 `path` 与 `name` 都非空

### 2.2路径检查

-后端只校验**非空**（`backend/src/handlers/project_directory.rs::create_project_directory`）

>文档之前说「必须以 `/`开头」「目录必须存在」是**错误**的：当前实现**只**做 `trim().is_empty()`检查，绝对路径/目录存在性**不**在创建时强制校验。

---

## 3.Todo工作流

### 3.1TodoDrawer选择 workspace

新建/编辑 Todo时，「**工作目录**」下拉框：

-选项 =项目目录白名单全部
-选完存到 `todo.workspace`字段

### 3.2执行时

-执行器拿到 Todo后，ntd后端 cd 到 `todo.workspace`
- 如果 workspace已被删/重命名 →执行器报错
-解决：在项目目录 Tab更新路径，或新建同名路径

---

## 4.与 Git Worktree

Todo还有一个 `worktree_enabled`字段：
-开启后，ntd在 `todo.workspace`下创建 git worktree（独立分支）跑任务
-适合「每个 Todo一个分支」的并行开发
-详见 [todo-lifecycle.md](../features/todo-lifecycle.md)

---

## 5.故障排查

### 5.1新增路径报「Path is required / Name is required」

- `path` 或 `name` 为空
-后端不会自动校验「目录是否存在」

### 5.2 Todo选了工作目录但跑不起来

- 看后端日志，搜 `workspace`关键字
- 检查目录权限：`chmod755 /path/to/dir`
-符号链接 /挂载点可能不被识别

### 5.3跨机器同步工作目录

-项目目录**不会**随云端同步走
-同步是 title + prompt级别的，路径保留本机
- 在新机器上要重新配置项目目录

---

## 6.相关 API

`project_directory::routes()`挂载在 `/api/project-directories` 下，标准 CRUD：

| Method | Path |用途 |
|--------|------|------|
| GET | `/api/project-directories` |列出全部 |
| POST | `/api/project-directories` |新增（body `{path, name}`，`name`必填） |
| PUT | `/api/project-directories/{id}` |修改（body `{name}`） |
| DELETE | `/api/project-directories/{id}` |删除 |
