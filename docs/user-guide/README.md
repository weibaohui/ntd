# NTD 使用说明

本目录是 **NTD (Nothing Todo)** 的终端用户使用说明，覆盖设置页面的 13 个 tab、主界面功能模块、运维操作以及常见问题。

## 快速上手

新用户推荐按以下顺序阅读：

1. [快速开始 - 安装与首次运行](getting-started/installation.md)
2. [快速开始 - 开发与生产环境](getting-started/dev-vs-prod.md)
3. [功能 - Todo 生命周期](features/todo-lifecycle.md)
4. [设置 - 执行器管理](settings/executors.md)（必须先配好至少一个执行器才能用）
5. [设置 - 系统设置](settings/system-settings.md)

## 设置页面（13 个 Tab）

| Tab | 文档 | 关键概念 |
|-----|------|----------|
| 系统设置 | [settings/system-settings.md](settings/system-settings.md) | 端口、数据库、日志、时区、并发、SLASH 命令 |
| 执行器管理 | [settings/executors.md](settings/executors.md) | 8 个执行器、自动检测、AI 使用统计 |
| 标签管理 | [settings/tags.md](settings/tags.md) | 标签 CRUD |
| 项目目录 | [settings/project-directories.md](settings/project-directories.md) | workspace 白名单 |
| 备份与恢复 | [settings/backup-and-restore.md](settings/backup-and-restore.md) | 数据库 / Todo / Skills 三类备份 |
| 运行管理 | [settings/runtime-management.md](settings/runtime-management.md) | 实时运行任务、批量停止 |
| 消息（飞书） | [settings/messages-feishu.md](settings/messages-feishu.md) | Bot 绑定、群白名单、推送、历史消息 |
| Session 管理 | [settings/sessions.md](settings/sessions.md) | 跨执行器会话 |
| 模板管理 | [settings/templates.md](settings/templates.md) | 本地模板 + 远程订阅 |
| Webhook | [settings/webhooks.md](settings/webhooks.md) | 外网触发、定向 todo |
| **云端同步** | **[settings/cloud-sync.md](settings/cloud-sync.md)** | **冲突策略、推送/拉取、Dry Run** |
| 关于 | [settings/about-and-upgrade.md](settings/about-and-upgrade.md) | 版本检查、一键升级 |

## 主界面功能

- [Todo 生命周期](features/todo-lifecycle.md) — 创建、状态机、Chat 视图
- [看板](features/kanban-board.md) — 按状态分列拖拽
- [仪表盘](features/dashboard.md) — 关键指标、Token 趋势
- [关系图](features/relation-map.md) — Todo 关联图谱
- [纪念板](features/memorial-board.md) — 已完成 Todo 展示
- [Skills 管理](features/skills-overview.md) — 总览/对比/同步/追踪
- [AI 使用统计](features/ai-usage-stats.md)
- [Webhook & 自动化](features/webhooks-and-automations.md)

## 运维

- [备份策略](operations/backup-strategy.md)
- [日志清理](operations/log-cleanup.md)
- [数据库优化](operations/database-optimize.md)
- [故障排查](operations/troubleshooting.md)

## 附录

- [术语表](appendix/glossary.md)
- [常见问题](appendix/faq.md)

---

## 文档约定

- **绝对路径引用**：所有源码路径用 `frontend/src/...` 或 `backend/src/...` 形式
- **API 路径**：HTTP API 用 `/api/...`，触发类 URL 用 `/webhook/...`
- **示例 token**：示例中 token 形如 `ntd_xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx`
- **截图位置**：待补充的截图统一放 `docs/user-guide/assets/<feature>/`

## 相关文档（已有）

- [NTD API 文档](../ntd-api.md) — 全部 HTTP API
- [NTD CLI 文档](../ntd-cli.md) — `ntd daemon` 等命令
- [前端功能清单](../frontend-features.md) — 短表格式的功能索引
- [Hook 系统设计](../hook-system-design.md) — 前置/后置 hook 机制
- [Session 管理设计](../session-management-design.md) — Session 抽象由来
- [架构总览](../ARCHITECTURE_HEALTH_CHECK_REPORT.md) — 后端模块划分
