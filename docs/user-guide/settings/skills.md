# Skills 管理（设置）

> **位置**：设置 → Skills 管理
> **详细文档**：[features/skills-overview.md](../features/skills-overview.md)

本 Tab 提供 4 个子视图，统一管理各执行器的预制 prompt 模板（Skill）。完整说明见 [features/skills-overview.md](../features/skills-overview.md)，本文只列入口与差异点。

## 4 个子视图

| 子视图 | 主要用途 | 详细章节 |
|--------|----------|----------|
| Skills 总览 | 列出 10 个来源的 skills，支持导入 / 导出 / 查看 SKILL.md | [§2.1](../features/skills-overview.md#21-skills-总览overview) |
| 对比分析 | 横向对比 10 个来源同名 skill 的差异 | [§2.2](../features/skills-overview.md#22-对比分析comparison) |
| 同步管理 | 单个 skill 跨执行器复制 | [§2.3](../features/skills-overview.md#23-同步管理sync) |
| 调用追踪 | skill 调用记录分页 | [§2.4](../features/skills-overview.md#24-调用追踪tracking) |

## 与执行器的关系

- **9 个真实执行器**（`claudecode` / `codebuddy` / `opencode` / `atomcode` / `hermes` / `kimi` / `mobilecoder` / `codex` / `pi`）有 skills 目录映射，可写
- `codewhale` 是执行器但**没有** skills 目录映射
- `agents` 是只读来源，扫描但不参与 Todo 执行；不出现于「执行器管理」标签

详细规则与同步边界见 [features/skills-overview.md](../features/skills-overview.md)。
