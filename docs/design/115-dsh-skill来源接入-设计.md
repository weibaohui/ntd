# 0. 文件修改记录表

| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| AI | 2026-08-25 | 初始版本 |

# 1. 背景（Why）

用户本机安装了 dsh CLI（DeepSeek Harness，`~/.local/bin/dsh`），其 skill 目录 `~/.dsh/skills` 下已有多个标准 SKILL.md 格式的 skill（含 assets/references/resources/scripts 子目录）。

ntd 的 Skills 统一管理目前覆盖 13 个执行器来源 + 1 个只读来源 `agents`（`~/.agents/skills`），用户无法在 ntd 中查看、对比、同步或管理 dsh 的 skill。

# 2. 目标（What，必须可验证）

- [ ] dsh 作为**可写 skill 来源**接入 Skills 统一管理：总览/对比/版本检测/同步（源与目标）/导出/删除/导入全部可用
- [ ] `ntd skill install` 默认把 ntd-usage 装入 `~/.dsh/skills/ntd-usage`
- [ ] dsh 不出现在执行器选择 UI（新建任务、批量换执行器、工作空间默认执行器、@提及候选）
- [ ] marketplace（bundled 技能）安装目标可选 dsh

# 3. 非目标（Explicitly Out of Scope）

- dsh **不加入** `ExecutorType` 枚举、`adapters::EXECUTORS` 注册表、日志事件解析器——dsh 不是 todo 执行器
- 不实现用 dsh 执行 todo、不解析 dsh session 日志
- 不改动 `agents` 只读保护语义（`is_readonly_skill_source` 仍只匹配 agents）
- 不为 dsh 编写 executorInstallPrompts 安装引导

# 4. 使用场景 / 用户路径

1. 用户打开 ntd → Skills 页 → 总览 Tab 出现「Dsh」页签，列出 `~/.dsh/skills` 下 5 个 skill；点开可看 SKILL.md 内容与文件列表
2. 用户在某个 skill 详情里点「同步」，目标勾选 Dsh，skill 复制到 `~/.dsh/skills/<name>`
3. 用户对 `~/.dsh/skills` 下的 skill 点「删除」→ 确认后目录被删除（dsh 可写，与 agents 不同）
4. 用户点「导入」选 zip → 解到 `~/.dsh/skills/<name>`
5. 终端执行 `ntd skill install` → 输出含 `✓ Installed ntd-usage skill for dsh`

# 5. 功能需求清单（Checklist）

- [ ] 后端：`executor_skills_dir_str` 加 `"dsh" => ~/.dsh/skills` 映射
- [ ] 后端：`ALL_SKILL_SOURCES` 加 `"dsh"`（list/compare/version-update 自动覆盖）
- [ ] 后端：`executor_label_for_source` 加 `"dsh" => "Dsh"`
- [ ] 后端：`delete_skill` / `import_skill` 的执行器校验从 `parse_executor_type` 改为 `executor_skills_dir_str`（否则非 ExecutorType 来源被 400 拒绝）
- [ ] 后端：`sync_skill` target 的 agents 字面量分支泛化为非 ExecutorType 来源集合 `["agents", "dsh"]`
- [ ] 后端：`main.rs` `KNOWN_EXECUTORS` 追加 `"dsh"`（skill install 默认包含）
- [ ] 前端：`executors.tsx` EXECUTORS 加 dsh 条目（不 resumable）+ EXECUTOR_COLORS 加配色 + EXECUTORS_FOR_PICKER 集合排除 agents/dsh
- [ ] 前端：`SkillCardView.tsx` EXECUTOR_ORDER 加 `'dsh'`
- [ ] 前端：`TodoDrawer.tsx` 执行器下拉初始回退值改用 EXECUTORS_FOR_PICKER（顺手修 agents 同款泄漏）
- [ ] 测试：后端 4 条守卫（label/非只读/目录映射/ALL_SKILL_SOURCES 含 dsh）+ 前端 picker 排除与撞色断言

# 6. 约束条件

- 遵循现有零告警策略：`cargo clippy --all-targets -- -D warnings`、`npx tsc --noEmit` 零错误
- 所有新增/修改代码带「为什么」注释
- dsh 的写操作（delete/import）复用现有路径安全校验（canonicalize、直接子目录检查、staging 原子替换），不新写安全逻辑
- 前端 dsh 配色须与现有 14 色不撞（有撞色断言测试先例）

# 7. 可修改 / 不可修改项

- ❌ 不可修改：`ExecutorType` 枚举、`is_readonly_skill_source` 的 agents 匹配、`ALL_EXECUTORS`（cfg(test)）计数
- ✅ 可调整：dsh 的显示名大小写（"Dsh"）、具体配色值

# 8. 接口与数据约定

无新 API。既有 `/api/v1/skills*` 系列接口的 `executor` 参数取值域扩展一个合法值 `"dsh"`：

- `GET /api/v1/skills` 响应数组新增 `{ executor: "dsh", executor_label: "Dsh", skills_dir: "~/.dsh/skills", ... }`
- `DELETE /api/v1/skills?executor=dsh&skill_name=X` → 200（与 agents 的 400 不同）
- `POST /api/v1/skills/import?executor=dsh` → 200
- `POST /api/v1/skills/sync` 的 `target_executors` 可含 `"dsh"`

# 9. 验收标准

- 如果 `GET /api/v1/skills`，则响应含 dsh 来源且 skills 为 `~/.dsh/skills` 下真实条目
- 如果对 dsh 调 DELETE，则对应目录被删除且返回 200（而非 "Unknown executor" 400）
- 如果对 dsh 调 IMPORT，则 skill 目录出现在 `~/.dsh/skills` 下
- 如果 sync 目标含 dsh，则复制成功、无 "Unknown target executor" 错误
- 如果执行 `ntd skill install`，则 `~/.dsh/skills/ntd-usage` 存在
- 如果打开新建任务弹窗，则执行器选项不含 Dsh/Agents
- 如果打开 Skills 总览，则出现 Dsh Tab；卡片视图显示 Dsh 安装方块

# 10. 风险与已知不确定点

- `TodoDrawer` 初始回退态泄漏为顺手修复，若审查认为超范围可单独拆出（改动一行，风险极低）
- `docs/user-guide/features/skills-overview.md` 来源表中 agents「禁止作为同步目标」描述已过时（023 已放行），本次一并修正
- dsh 目录若被 dsh CLI 自身管理（类似 plugin 机制），ntd 侧删除/导入与之并发的行为未定义——接受文件系统层面的最终一致（与其他执行器目录同待遇）
