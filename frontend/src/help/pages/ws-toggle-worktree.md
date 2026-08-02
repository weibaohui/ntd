# Git Worktree / 自动清理开关切换

## 功能位置
工作空间页 → 工作空间卡底部署理区 →「Git Worktree」`Switch` /「自动清理」`Switch`

## 数据流图（前端 → 后端）

```mermaid
flowchart LR
  User["切换 Switch"] --> handleToggleWorktree["handleToggleWorktree(id, flag, next)"]
  handleToggleWorktree --> guardCheck{flag=autoCleanup 且 next 且 !git_worktree_enabled?}
  guardCheck -->|"是"| warn["message.warning 请先开启 Worktree"]
  guardCheck -->|"否"| optimistic["乐观更新 setProjectDirectories"]
  optimistic -->|"db.updateProjectDirectory(id, name, options)"| API["PUT /api/v1/project-directories/{id}"]
  API -->|"成功"| Done["保持乐观值"]
  API -->|"失败"| rollback["setProjectDirectories 回滚 previous"]
```

## 调用关系链路图

```mermaid
flowchart TD
  Panel["ProjectDirectoriesPanel.tsx<br>ProjectDirectoriesPanel()"] --> WorktreeSwitch["Git Worktree Switch<br>onChange handleToggle(id,'gitWorktreeEnabled',v)"]
  Panel --> AutoCleanupSwitch["自动清理 Switch<br>onChange handleToggle(id,'autoCleanup',v)<br>disabled=!git_worktree_enabled"]
  WorktreeSwitch --> handleToggleWorktree["handleToggleWorktree"]
  AutoCleanupSwitch --> handleToggleWorktree
  handleToggleWorktree --> guardCheck["flag=autoCleanup && next && !git_worktree_enabled → 前端拦截"]
  handleToggleWorktree --> computeNext["nextGit / nextAuto<br>关 git 时 auto 联动复位 false"]
  computeNext --> optimistic["setProjectDirectories 乐观更新"]
  optimistic --> db1["db.updateProjectDirectory(id, target.name, {gitWorktreeEnabled, autoCleanup})"]
  db1 --> rollbackOnError["catch → setProjectDirectories 回滚 previous"]
```

## 数据结构图

```mermaid
classDiagram
  class ProjectDirectory {
    id: number
    git_worktree_enabled: boolean
    auto_cleanup: boolean
  }
  note for ProjectDirectory "auto_cleanup 强依赖 git_worktree_enabled<br>关 worktree 时 auto_cleanup 联动复位 false"
```

## 数据变更图

```mermaid
stateDiagram-v2
  [*] --> Idle: 卡片展示当前策略
  Idle --> Optimistic: 切换 Switch
  Optimistic --> Idle: update 成功保持乐观值
  Optimistic --> Idle: update 失败回滚 previous
  Idle --> WarnBlocked: 开 auto 但 worktree 未开
  WarnBlocked --> Idle: message.warning
```

## 开发指导
- **前端入口**：`frontend/src/components/settings/ProjectDirectoriesPanel.tsx` 的 `handleToggleWorktree` 回调（`flag` 参数为 `'gitWorktreeEnabled'` 或 `'autoCleanup'`）
- **后端入口**：`backend/src/handlers/project_directory.rs` 处理 `PUT /api/v1/project-directories/{id}`，body 含 `git_worktree_enabled` / `auto_cleanup` 可选字段
- **注意**：前端先拦一道「开 auto 但关 worktree」的废组合避免无谓 HTTP；`auto_cleanup` 在 `git_worktree_enabled` 关闭时联动复位为 `false`；乐观更新失败需回滚 `previous`
- **扩展**：新增工作空间级策略开关时后端 schema migration + handler body 追加字段，前端 `handleToggleWorktree` 的 `flag` union 扩展并加乐观更新逻辑
