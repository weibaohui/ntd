# 115 — dsh skill 来源接入实现总结

> 对应设计文档：`docs/design/115-dsh-skill来源接入-设计.md`
> 分支：`feat/dsh-skill-source`

# 1. 交付内容

dsh（DeepSeek Harness CLI，`~/.local/bin/dsh`）的 skill 目录 `~/.dsh/skills` 以**可写 skill 来源**形态接入 Skills 统一管理：总览/对比/版本检测/同步（源与目标）/导出/删除/导入全部可用；`ntd skill install` 默认包含 dsh；执行器选择 UI 不出现 Dsh。

# 2. 代码改动

## 2.1 后端 `backend/src/handlers/skills.rs`

| 位置 | 改动 | 为什么 |
|------|------|--------|
| `executor_skills_dir_str` | 加 `"dsh" => ~/.dsh/skills` | 名称→目录唯一映射点，读路径（list/content/file/export）自动生效 |
| 同上 | **顺手补齐** `"codewhale"`/`"kilo"` 映射 | 存量缺口：PR #428（CodeWhale）与 Kilo 引入时漏加映射，导致这两个执行器在 Skills 管理/对比/`ntd skill install` 一直不可见（已用 git 历史核实为存量问题，非本 PR 引入） |
| `ALL_SKILL_SOURCES` | 加 `"dsh"` + 补 `"codewhale"`/`"kilo"` | list/compare/version-update 三接口的来源枚举 |
| `executor_label_for_source` | 加 `"dsh" => "Dsh"` | 非 ExecutorType 来源的显示名分支 |
| `delete_skill` / `import_skill` | `parse_executor_type + executor_skills_dir` → `executor_skills_dir_str` | 原链路对非 ExecutorType 来源一律 400；改字符串映射后 dsh 可写，未知名字仍 400（安全性不降级） |
| `sync_skill` target 分支 | agents 字面量分支泛化为 `NON_EXECUTOR_SOURCES: &[&str] = &["agents", "dsh"]` | dsh 可作同步目标；后续新增非执行器来源只加一项 |

## 2.2 后端 `backend/src/main.rs`

- `KNOWN_EXECUTORS` 追加 `"dsh"`：`ntd skill install` 默认装入（agents 维持仅 `--all` 注入的现状，dsh 是可写来源与执行器一致）。

## 2.3 前端

| 文件 | 改动 | 为什么 |
|------|------|--------|
| `utils/executors.tsx` | EXECUTORS 加 dsh 条目（无 resumable）+ EXECUTOR_COLORS 加 `dsh: '#7c3aed'` + `EXECUTORS_FOR_PICKER` 改集合排除 `{agents, dsh}` | dsh 需要 label/颜色才能在 Skills UI 正确显示；picker 排除防止泄漏到 5 处执行器选择 UI |
| `components/skills/SkillCardView.tsx` | `EXECUTOR_ORDER` 加 `'dsh'` | 卡片视图的来源方块顺序 |
| `components/TodoDrawer.tsx` | 执行器下拉初始回退值 `EXECUTORS` → `EXECUTORS_FOR_PICKER` | 后端配置为空时 agents/dsh 不再泄漏进 Todo 执行器下拉（顺手修 agents 同款问题） |

## 2.4 测试

- 后端 4 条新守卫：`executor_label_for_source("dsh")=="Dsh"`、`executor_skills_dir_str("dsh")` 映射正确、`ALL_SKILL_SOURCES` 含 dsh、`!is_readonly_skill_source("dsh")`。
- 前端 3 条新断言：dsh 条目存在且非 resumable、dsh 颜色与全部现有来源不撞色、`EXECUTORS_FOR_PICKER` 不含 dsh。

# 3. 验证结果

| 验证项 | 结果 |
|--------|------|
| `cargo clippy --all-targets -- -D warnings` | ✅ 零告警 |
| `cargo test`（全量） | ✅ 2283 passed / 0 failed |
| `npx tsc --noEmit` | ✅ 零错误 |
| `npm run build` | ✅ 成功（chunk 大小告警为存量） |
| `npm test`（vitest 全量） | ✅ 54 文件 455 用例全过 |
| `GET /api/v1/skills` | ✅ 15 来源，dsh 5 个 skill（label/dir/exists 正确） |
| `GET /api/v1/skills/compare` | ✅ dsh 列存在，5 个 skill 命中 |
| sync pi→dsh / content / export | ✅ 同步落盘、内容读取、zip 导出（6081 字节）正常 |
| import zip→dsh / delete | ✅ 导入成功、删除成功（200，非 Unknown executor） |
| agents 只读守卫回归 | ✅ DELETE agents 仍返回 400 只读保护 |
| `ntd skills install` | ✅ `✓ Installed ntd-usage skill for dsh (1 files)` |
| UI（agent-browser） | ✅ 总览 Dsh Tab（5）；Dsh 筛选生效；详情抽屉删除/同步按钮可用；同步 Modal 目标 14 项（Dsh 作为源时自身排除）；Dsh→Kilo 同步落盘成功；新建任务委派执行器下拉 12 项无 Dsh/Agents |

测试期间的临时数据（`~/.dsh/skills/ntd-usage`、`test-import-115`、`~/.kilo/skills/excel-dcf-modeler`）已全部清理，`~/.dsh/skills` 恢复原始 5 个 skill。

# 4. 环境修复记录（非代码改动）

验证时发现 `~/.ntd/config.dev.yaml` 被配置为 `port: 8088` + `db_path: data.db`（生产端口+生产库），与项目约定的 dev 环境（18088 + data.dev.db）不符，导致 `make dev` 与生产进程抢 8088 端口且存在误写生产库风险。已修正为 18088/data.dev.db（原文件备份为 `config.dev.yaml.bak-115`）。

# 5. 遗留说明

- `executor_skills_dir_str` 现含 15 个来源映射；`codewhale`/`kilo` 补齐后 Skills 总览/对比/`ntd skill install` 首次完整覆盖全部 13 个执行器 + agents + dsh。
- `SkillDetailDrawer` 删除按钮对只读来源也无条件渲染（靠后端 400 报错），这是 agents 时代既有行为，未在本次范围调整。
