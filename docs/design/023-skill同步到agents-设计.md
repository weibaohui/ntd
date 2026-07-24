# 0. 文件修改记录表

| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| AI | 2026-07-24 | 初始版本 |

# 1. 背景

`agents` 是一个特殊的 skill 来源（目录 `~/.agents/skills`），之前被设计为**只读**：可以从 agents 读取 skill、复制到其他执行器，但不能把 skill 写回到 agents。

现在需要改变这一限制：允许 skill 同步到 `agents` 目录，使 `agents` 成为一个可写目标。

# 2. 改动范围

| 层 | 文件 | 改动 |
|----|------|------|
| 后端 | `backend/src/handlers/skills.rs` | `sync_skill()` 中放行 `agents` 作为目标，同时保留 `delete`/`import` 的只读保护 |
| 后端 | `backend/src/handlers/skills.rs` | `sync_skill()` 中为 `agents` 添加特殊路径解析（`agents` 不在 `ExecutorType` 枚举中） |
| 前端 | `frontend/src/components/skills/SkillDetailDrawer.tsx` | 移除 `agents` checkbox 的禁用标记 |
| 前端 | `frontend/src/components/skills/SkillMarketplace.tsx` | 检查是否有类似过滤并移除 |

# 3. 具体方案

## 3.1 后端 sync_skill 放行 agents

当前代码在 `sync_skill()` 中（1301-1307行）：

```rust
if is_readonly_skill_source(target) {
    errors.push(...);
    continue;
}
```

改为：

```rust
// agents 过去是只读来源，现在允许作为同步目标。
// 但 delete/import 等其他写操作仍保持只读保护。
```

同时在 1308 行的 `parse_executor_type(target)` 之前增加对 `"agents"` 的特殊处理，因为 `agents` 不在 `ExecutorType` 枚举中，直接用 `executor_skills_dir_str("agents")` 获取目标路径。

## 3.2 前端移除 agents 禁用

`SkillDetailDrawer.tsx` 的 257-258 行：

```tsx
// agents 是只读来源，不能作为同步目标
const isReadonly = exec.value === 'agents';
```

将 `isReadonly` 变量改为仅用于其他只读条件（或无条件），让 agents 的 checkbox 可选。

## 3.3 SkillMarketplace.tsx 检查

检查 SkillMarketplace 的目标选择器是否有类似过滤，如有同步移除。

# 4. 不变的内容

- `is_readonly_skill_source()` 函数仍然保留，仅用于 `delete_skill` 和 `import_skill` 的守卫
- `EXECUTORS_FOR_PICKER` 仍然排除 `agents`（agents 没有 CLI，不出现在 Todo 执行器选择中）
- `agents` 不在后端 `EXECUTORS` 数组和 `ExecutorType` 枚举中 — 只影响 skill 同步

# 5. 风险

- `~/.agents/skills` 目录由外部系统维护，向其中写入 skill 可能导致冲突。用户应知晓这是一个「复制安装」操作，不是只读引用。
- 如果 agents 目录下的 skill 与同名 skill 冲突，同步操作会覆盖。需要跟其他执行器的行为保持一致。
