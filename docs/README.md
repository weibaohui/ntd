# 项目文档目录索引

> 本文档定义 `docs/` 目录的职责划分与命名规范，所有 AI 与开发者在创建或变更文档时必须遵守。
>
> 完整规范请参考：
> - `docs/文档编写规范/README.md` — AI 文档读写与存放规则
> - `docs/文档编写规范/需求编写规范.md` — AI 可执行需求规范
> - `docs/文档编写规范/Bug缺陷处理文档编写规范.md` — Bug 缺陷文档规范
> - `AGENTS.md` — AI 协作开发约定

---

## 标准目录结构

```
docs/
├── bugs/                    # 缺陷问题处理记录
│   └── <编号>-<标题>/
│       ├── <编号>-<标题>-缺陷说明.md
│       ├── <编号>-<标题>-缺陷分析.md
│       └── <编号>-<标题>-修复总结.md
├── requirements/            # 需求 & 实现总结
│   ├── <编号>-<功能名称>-需求.md
│   └── <编号>-<功能名称>-实现总结.md
├── design/                  # 设计与实现方案
│   └── <编号>-<功能名称>-设计.md
├── testing/                 # 测试设计文档
│   └── <编号>-<功能名称>-测试.md
├── decisions/               # 架构与关键决策（ADR）
│   └── ADR-<编号>-<标题>-决策.md
│
├── 开发规范/                 # 前后端开发规范
│   ├── 后端规范/             # Rust 后端规范（编号 01-15）
│   └── 前端规范/             # React 前端规范（编号 01-14）
├── 文档编写规范/             # 文档编制规范（读写规则、需求规范、Bug 规范）
│
├── user-guide/              # 用户指南（迁移中，已有内容）
├── samples/                 # 示例配置/模板（迁移中，已有内容）
└── design-system/           # 设计系统（迁移中，已有内容）
```

## 文档命名规范

所有需求相关文档必须使用统一编号：

```
<编号>-<功能名称>-<文档类型>.md
```

示例：
- `001-日志AI分析-需求.md`
- `001-日志AI分析-设计.md`
- `001-日志AI分析-测试.md`
- `001-日志AI分析-实现总结.md`

### 各子目录文档类型

| 目录 | 文档类型 | 说明 |
|------|---------|------|
| `bugs/` | 缺陷说明、缺陷分析、修复总结 | 每个缺陷一个子目录 |
| `requirements/` | 需求、实现总结 | 功能需求与完成后总结 |
| `design/` | 设计 | 实现方案与技术设计 |
| `testing/` | 测试 | 测试设计与验收要点 |
| `decisions/` | 决策 | 架构或关键技术决策（ADR 格式） |

## 文档读取顺序

AI 在执行某个功能时（编号 X），必须按以下顺序读取文档：

1. `docs/requirements/X-*-需求.md`
2. `docs/design/X-*-设计.md`（如存在）
3. `docs/testing/X-*-测试.md`（如存在）
4. `docs/decisions/` 下与该功能相关的决策文档

> ❗ 若需求文档不存在，AI 必须停止执行并提示人类。

## 已有文档清单

### docs/design/（21 份）

| 编号 | 文件 | 原名 |
|------|------|------|
| 001 | API路由重构-设计.md | api-routing-redesign.md |
| 002 | 黑板系统-设计.md | blackboard-design.md |
| 003 | 黑板Wiki集成-设计.md | blackboard-wiki-design.md |
| 004 | 黑板开发计划-设计.md | blackboard-dev-plan.md |
| 005 | 专家系统-设计.md | expert-system-design.md |
| 006 | TODO中心-设计.md | todo-center-design.md |
| 007 | 操作按钮组件-设计.md | action-button-component.md |
| 008 | CLI设计-设计.md | CLI_DESIGN.md |
| 009 | 双事件解析器分析-设计.md | dual-event-parser-analysis.md |
| 010 | 执行事件统一设计-设计.md | execution-events-unified-design.md |
| 011 | HelpCard重构-设计.md | help-card-redesign.md |
| 012 | Hook系统-设计.md | hook-system-design.md |
| 013 | Loop异常处理-设计.md | loop-abnormal-handler-todo.md |
| 014 | Loop黑板CLI-设计.md | loop-blackboard-cli.md |
| 015 | Loop流程控制-设计.md | loop-flow-control-design.md |
| 016 | NTDConnect-设计.md | ntd-connect-design.md |
| 017 | NTDConnect试运行迁移-设计.md | ntd-connect-dry-run-migration.md |
| 018 | 飞书消息通知-设计.md | plan-feishu-messaging.md |
| 019 | 会话管理-设计.md | session-management-design.md |
| 020 | Webhook实体分析-设计.md | webhook-per-entity-analysis.md |
| 021 | 工作空间重构-设计.md | WORKSPACE_REFACTOR_ANALYSIS.md |
| 999 | 架构总览-设计.html | architecture.html |

### docs/requirements/（3 份）

| 编号 | 文件 | 原名 |
|------|------|------|
| 001 | 项目规格说明-需求.md | SPEC.md |
| 002 | 功能特性总览-需求.md | FEATURES.md |
| 003 | 前端功能特性-需求.md | frontend-features.md |

### docs/decisions/（4 份）

| 编号 | 文件 | 原名 |
|------|------|------|
| ADR-001 | 架构健康检查-决策.md | ARCHITECTURE_HEALTH_CHECK_REPORT.md |
| ADR-002 | 优化建议-决策.md | OPTIMIZATION_RECOMMENDATIONS.md |
| ADR-003 | 代码质量审计-决策.md | code-quality-audit-2026-07.md |
| ADR-004 | 死代码扫描-决策.md | dead-code-scan.md |

### docs/bugs/（2 个缺陷）

| 缺陷编号 | 文件 | 原名 |
|---------|------|------|
| 001 | 编译指令pragma优化API-缺陷说明.md | issue_295_pragma_optimize_api_issue.md |
| 002 | Antd下拉框弹窗修复-缺陷说明.md | ANTD_DROPDOWN_FIX.md |

### docs/user-guide/（迁移完成）

| 子目录 | 文件 | 原名 |
|--------|------|------|
| operations/ | executor-integration-guide.md | ADD_EXECUTOR_GUIDE.md |
| operations/ | npm-publish.md | NPM_PUBLIST.md |
| features/ | action-button-usage.md | action-button-usage-guide.md |
| reference/ | api-reference.md | ntd-api.md |
| reference/ | cli-reference.md | ntd-cli.md |

### 其他已有目录

| 目录 | 说明 |
|------|------|
| `user-guide/` | 用户指南（含 getting-started / features / operations / settings / appendix） |
| `samples/` | 各 AI 工具的多 Agent 测试样本 |
| `design-system/` | 设计系统参考 |

> 根目录仅剩 `README.md` 和 4 张遗留截图（`dashboard.png`、`detail.png`、`info.png`、`kanban.png`），截图不宜提交到 Git，因已在历史中不再清理。
