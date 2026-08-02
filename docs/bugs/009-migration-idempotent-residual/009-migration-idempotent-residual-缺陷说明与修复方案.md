# 009 - 迁移幂等性：残留态 DB 启动自愈

| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| AtomCode (GLM-5.2) | 2026-08-02 | 初始版本 |

关联 Issue: #973
关联 PR: (待开)

## 1. 缺陷说明

本地执行 `ntd server start` 时，数据库迁移报错导致后端无法启动：

```
WARN ntd::db::migration: migration v1: ALTER TABLE todos ADD COLUMN kind TEXT NOT NULL DEFAULT 'item': Execution Error: error returned from database: (code: 1) duplicate column name: kind
WARN ntd::db::migration: migration v1: ALTER TABLE loops ADD COLUMN workspace TEXT: Execution Error: error returned from database: (code: 1) duplicate column name: workspace
ERROR ntd: Failed to open database db_path=/Users/mac/.ntd/data.db error=Execution Error: error returned from database: (code: 1) no such column: acceptance_criteria
Failed to open database at /Users/mac/.ntd/data.db: Execution Error: error returned from database: (code: 1) no such column: acceptance_criteria
```

DB 路径：`/Users/mac/.ntd/data.db`。

## 2. 缺陷分析

### 2.1 根因链

1. `add_column_warn`（`db/migration/mod.rs:215`）对「列已存在」只 `warn` 不阻断 → `kind`/`workspace` 的 `duplicate column` 只是 WARN，**不致命**
2. `run_migrations`（`db/mod.rs:221`）按版本顺序跑，每个迁移 `m.up(self).await?` 用 `?` —— 任一迁移返回 Err 就整个 `init_tables` 失败、`Database::new` 失败、`server start` 退出
3. 本地 DB 处于「中间版本残留态」：`schema_version` 表里**缺**某些版本的行（迁移半途中断过），导致 `run_migrations` 重跑时某个迁移 `up` 里**依赖 `acceptance_criteria` 列已存在**的 SQL（SELECT/INSERT 该列）直接抛 `no such column` → Err → 启动失败

### 2.2 为什么会进入残留态

- 旧版本迁移断电/panic 中断：表结构改了一半但 `schema_version` 行未写
- 跨大版本本地试跑后回退：DB 残留新列，但 `schema_version` 被回退覆盖
- 手动改过 DB：人为删列/删行破坏一致性

### 2.3 现有幂等手段不足

| 手段 | 位置 | 不足 |
|---|---|---|
| `add_column_warn` | mod.rs:215 | 只对 ADD COLUMN 兜住「列已存在」；**不兜**「列缺失导致后续 SQL 失败」 |
| `add_column_if_missing` | mod.rs:202 / v72 / v73 | 新迁移用，但**旧迁移 v1 的 `add_legacy_todos_columns` 没用**，仍是裸 `add_column_warn` |
| `IF NOT EXISTS` | v1 feishu_messages 分支 | 仅 SQLite 3.35+，且只覆盖 3 列 |

真正要修的是：让迁移在「列已存在/缺失」的残留态下也能幂等自愈，而不是硬抛。

## 3. 修复方案

### 3.1 方案 A（推荐）：新增 v86 自愈迁移

在迁移链末端追加 `v86`「残留态自愈」迁移，**幂等探测并补齐**历史缺失列：

- 探测 `todos.acceptance_criteria` 不存在 → `ALTER TABLE todos ADD COLUMN acceptance_criteria TEXT`
- 探测 `todos.kind` 不存在 → 补
- 探测 `loops.workspace` 不存在 → 补
- 其他历史 `add_legacy_*` 列同理幂等补齐

**优点**：
- 不动旧迁移 v1，幂等性「向前修」而非改历史，符合迁移不可变约定
- 集中一处自愈逻辑，后续残留态都能兜
- 有 `schema_version` 行的正常库：`v86.up` 探测列都在 → no-op → 记行 → 完成
- 残留态库：`v86.up` 探测缺列 → 补 → 记行 → 完成，启动恢复正常

**实现**：复用 `add_column_if_missing`（mod.rs:202 已有）。

### 3.2 方案 B（弃）：改旧迁移 v1

把 v1 的裸 `add_column_warn` 全换成 `add_column_if_missing`。

**弃因**：迁移不可变约定——已应用到生产库的迁移不应改，否则 `schema_version` 已记行但代码变了，幂等性反退。

### 3.3 方案 C（弃）：DB 重建脚本

提供 `ntd db reset` 命令清库重建。

**弃因**：丢用户数据，治标不治本。

## 4. 实施计划

### 4.1 v86 自愈迁移

- [ ] 新建 `backend/src/db/migration/v86.rs`，实现 `V86SelfHealResidual`
- [ ] `up`：幂等探测并补齐 `todos` / `loops` / 其他历史表的关键缺失列
- [ ] 在 `db/migration/mod.rs::all_migrations()` 注册 v86
- [ ] 单元测试：
  - 正常库跑 v86 → no-op，行被记
  - 残留态库（删列模拟）跑 v86 → 列补回，行被记
  - v86 幂等：连跑两次第二次 no-op

### 4.2 验证

- [ ] `cargo clippy --all-targets -- -D warnings` 零告警
- [ ] `cargo test --lib` 全过（含新增 v86 测试）
- [ ] （可选）用本地残留态 `data.db` 实跑 `ntd server start` 确认能起

## 5. 验收标准

| AC | 内容 |
|---|---|
| AC-009-1 | 新增 v86 自愈迁移，注册到 all_migrations |
| AC-009-2 | 正常库跑 v86 为 no-op（不重复加列） |
| AC-009-3 | 残留态库（缺 `todos.acceptance_criteria`）跑 v86 后列补回，`server start` 可启动 |
| AC-009-4 | `cargo clippy` + `cargo test` 通过 |

## 6. 风险与对策

| 风险 | 对策 |
|---|---|
| v86 漏补某列 | 以 `add_legacy_*` 的列清单为准，逐表逐列幂等补 |
| v86 误改正常库 | 每列先 `pragma_table_info` 探测，存在则跳过 |
| 迁移顺序 | v86 放链末，确保所有旧迁移先跑完再自愈 |

## 7. YAGNI 自检

- ✅ v86 自愈迁移：必须，issue 报的就是残留态无法自愈
- ✅ 复用 `add_column_if_missing`：已有，零新逻辑
- ❌ 不改旧迁移 v1：迁移不可变约定
- ❌ 不加 `ntd db reset` 命令：YAGNI，v86 兜住即可
