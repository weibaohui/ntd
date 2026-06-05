# Skills 管理

> **位置**：设置 → Skills 管理
> **前端**：`frontend/src/components/SkillsPanel.tsx`（4 个子视图）
> **后端**：`backend/src/handlers/skills.rs`

Skills 是 ntd 帮各执行器管理的**预制 prompt 模板**，类似 Claude Code 的 `.claude/skills/`。每个执行器有自己的 skill 目录，ntd 统一扫描和同步。

## 0. Skill 来源

| 来源 | 目录 | 可写 | 说明 |
|------|------|------|------|
| `claudecode` / `codebuddy` / `opencode` / `atomcode` / `hermes` / `kimi` / `joinai` / `codex` | `~/.{executor}/skills/` | ✅ | 8 个真实执行器 |
| `agents` | `~/.agents/skills/` | ❌ | **只读来源**，扫描但不参与 Todo 执行 |

### 0.1 `agents` 只读来源

`agents` 是**只读** skill 来源（通常由 cc-connect 等其他工具维护），ntd：
- ✅ **扫描并展示** skills
- ✅ 支持**导出**单个 skill 为 zip
- ✅ 支持**作为同步源**（把 `agents` 的 skill 复制到其他执行器）
- ❌ 禁止**导入**到 `agents`（避免覆盖其他工具的内容）
- ❌ 禁止**删除** `agents` 里的 skill
- ❌ 禁止把 `agents` 当作**同步目标**
- ❌ 不出现在「执行器管理」和 TodoDrawer 下拉框

**使用场景**：本地有 cc-connect 等工具放在 `~/.agents/skills/` 的 skill，你想让 ntd 的某个执行器也能用 → 在「Skills 同步」选 source=agents、target=claudecode，复制一份过去。

## 1. 什么是 Skill

一个 Skill = 一个 `SKILL.md` 文件 + 可选子目录（脚本、模板等）。结构：

```
~/.claude/skills/
├── code-review/
│   ├── SKILL.md       # 主文件，包含 name + description + 触发 prompt
│   └── templates/
│       └── pr.md
└── bug-fix/
    └── SKILL.md
```

`SKILL.md` 内容：

```markdown
---
name: code-review
description: 审查 PR 代码，提供建议
---

请按以下步骤 review：
1. 检查代码风格
2. 检查潜在 bug
3. 检查性能问题
```

## 2. 4 个子视图

### 2.1 总览（Overview）

- 9 个来源 tab 切换（8 个执行器 + agents）
- 每个来源下列出它的所有 skills
- 单个 skill 详情（点开看 SKILL.md 全文）
- 「**导入**」按钮：上传 .zip 包（agents 不可导入）
- 「**导出**」按钮：把当前来源的所有 skills 打成 zip

### 2.2 对比（Comparison）

- 横向看 9 个来源**同名** skill 的差异
- 「共享」（多个来源都有）/ 「独占」（仅一个来源有）
- 适合发现「哪个执行器落后了」

### 2.3 同步（Sync）

- 选一个**源来源** + 多个**目标来源**
- 把源来源的所有 skills 复制到目标
- 覆盖目标已有的同名 skill
- 用于：把 `agents`（cc-connect）的 skill 同步到 Claude Code

agents 只能作为 source，不能作为 target（界面 disabled）。

### 2.4 追踪（Tracking）

- 所有 skill 调用记录分页
- 字段：哪个执行器、哪个 skill、什么时间、token 消耗
- 用于分析「哪些 skill 被频繁用」

## 3. 操作

| 操作 | 入口 | API |
|------|------|-----|
| 列出 skills | 总览 | `GET /api/skills` |
| 对比 | 对比 tab | `GET /api/skills/compare` |
| 同步 | 同步 tab | `POST /api/skills/sync` |
| 导入 | 总览 | `POST /api/skills/import` |
| 导出 | 总览 | `GET /api/skills/export` |
| 删 | 总览 | `DELETE /api/skills` |
| 看 SKILL.md 全文 | 总览 | `GET /api/skills/content` |
| 调用记录 | 追踪 | `GET /api/skills/invocations` |

## 4. 与 Todo 的关系

- 在 TodoDrawer 创建 Todo 时，可以选「**附加 Skills**」
- ntd 会把选中的 skill 的 prompt 拼到 Todo prompt 前面
- 这样 Todo 一跑就带上了 skills 的能力

## 5. 故障排查

### 5.1 扫描不到 skill

- skill 目录不在默认位置
- 改各执行器的 skill 根目录（看各执行器文档）
- 重启 ntd 重新扫描

### 5.2 SKILL.md 解析失败

- frontmatter 格式不对（YAML 头）
- 用 [yaml validator](https://www.yamllint.com/) 检查

### 5.3 同步后另一个执行器不识别

- 不同执行器对 skill 格式要求不同
- 同步时 ntd 只做文件复制，**不做格式转换**
- 看各执行器的 skill 文档

### 5.4 agents 看不到

- `~/.agents/skills` 目录不存在
- 创建空目录 `mkdir -p ~/.agents/skills` 即可

### 5.5 agents 删除按钮是灰的

- 这是设计：agents 是只读来源，不允许本地修改
- 误删风险：可能破坏 cc-connect 等工具的数据
- 解决：用「Skills 同步」把 agents 的内容复制到其他执行器

## 6. 备份

Skills 备份是 ntd 三大备份之一，详见 [backup-and-restore.md](../settings/backup-and-restore.md#3-skill-备份)。**注意**：agents 不会被备份工具自动管理，因为它不在 ntd 的控制范围内。
