# 093-kilo系执行器提炼基类-实现总结

| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| AI (Pi) | 2026-08-08 | 初始版本 |
| AI (zhanlu) | 2026-08-10 | 按 CodeRabbit 评审修正：差异方法口径（5 行为查询+1 类型映射）、"唯一事实源"限定为运行时行为差异、测试结果去掉 ✅ 并写明失败明细 |
| AI (zhanlu) | 2026-08-10 | 按 CodeRabbit 评审收窄安全反思：to_full_json 防覆盖属已声明的边界载荷语义变化，不再笼统写"无协议变化" |

> 对应设计：`docs/design/093-kilo系执行器提炼基类-设计.md`。093 专项 B1（重构批次 1），
> 消除 code-refactor 诊断的 Copy-Paste Programming 反模式（4 份 77%~83% 逐字相同的适配器）。

## 1. 实现了什么

| 变化 | 文件 | 行数 |
|------|------|------|
| 新增统一事件模型 | `adapters/step_event.rs`（+8 反序列化单测） | ~250 |
| 新增统一执行器 | `adapters/step_protocol.rs`（+21 统一单测） | ~640 |
| 删除四份复制粘贴适配器 | `kilo.rs` / `opencode.rs` / `zhanlu.rs` / `mimo.rs` | **-1840** |
| 事件模块缩为别名壳 | `{kilo,opencode,zhanlu,mimo}_event.rs`（extractor 等引用方零改动） | 各 ~15 |
| 调用点适配 | `adapters/mod.rs` 注册表、`log_capture.rs` NTD-012 回归测试、2 个集成测试文件 | — |

**净减 ~1800 行**；运行时行为差异知识从「散在 4 文件」收敛为 `StepProtocolFlavor` 的 6 个方法（5 个协议行为查询 + 1 个类型映射，是**运行时行为差异**的唯一事实源；序列化层差异由 `step_event.rs` 承载）。

## 2. 与设计的对应关系

| 设计项 | 落地 | 状态 |
|--------|------|------|
| C1 统一事件模型 | canonical `StepAgentEvent` 族；mimo camelCase 键走 serde `alias`；`snapshot` 并入；`Serialize` derive；`to_full_json` 采防覆盖语义（设计声明的唯一有语义对齐） | ✅ |
| C2 统一执行器 | `StepProtocolExecutor` + 4 命名构造器（`::kilo()`/`::opencode()`/`::zhanlu()`/`::mimo()`） | ✅ |
| C3 事件模块别名壳 | 4 个 `*_event.rs` 缩为 `pub use` 再导出，`execution_events/impls/{kilo,opencode}.rs` 等零改动 | ✅ |
| C4 调用点适配 | 注册表 4 处 + 3 个测试文件的构造器改名 | ✅ |

## 3. 关键实现点

- **差异矩阵逐项翻译成 flavor 查询方法**：`accepts_hyphenated_events` / `has_reasoning_event` / `serializes_full_tool_state` / `resume_falls_back_to_dash_c` / `reports_model` / `executor_type`——每格差异都有对应单测（hyphenated 接受矩阵、mimo reasoning、tool 载荷形态、-c 降级、get_model 差异）。
- **事件名归一化**：kilo 系连字符式折叠为下划线式统一匹配（`replace('-', "_")`），mimo 不归一化——未知名自然落空，语义与旧实现逐字对齐。
- **测试并集收敛**：4 套近重复测试（~86 例）合并为 29 例参数化套件（lib 测试总数 1713→1627 是预期去重，非覆盖丢失）；关键路径按 flavor 参数化跑多遍。

## 4. 测试与验证结果

- `cargo clippy --all-targets -- -D warnings`：零告警 ✅
- `cargo test --no-fail-fast`：1627 通过 / 1 失败。失败项为 `git_sync::tests::test_sync_repo_restores_deleted_file`——预存量环境问题（本机 git 版本过老不支持 `init -b`，main 上同样失败），与本次改动无关
- NTD-012 回归测试（改构造器后）继续通过 ✅
- `make dev` 启动零错误；`GET /api/v1/executors` 确认 kilo/mimo/opencode/zhanlu 注册正常 ✅

## 5. 已知限制

- `execution_events/impls/` 侧的 4 个 Extractor 仍有相似结构（kilo/mimo 差异 370/350 行是真差异，opencode/zhanlu 高度相似）——属 B4「双解析体系收敛」范围，本 PR 刻意不动。
- mimo 旧 `to_full_json` 与 kilo 系的语义差异（extra 同名键覆盖）统一为防覆盖版，属于声明过的唯一行为微差异。
- 别名壳标注「新代码请直接用 step_event」，后续 B4 时可随 extractor 收敛一并删除。

## 6. 安全反思

- 无公开 API、DB schema、进程协议（JSONL 事件格式）变化；JSONL 解析行为逐格保持（差异矩阵即核对清单）；
- **例外收窄**（CodeRabbit #1006 评审）：`to_full_json` 防覆盖对齐是已声明的边界载荷语义变化，
  仅影响 extra 含同名键的手工构造场景，真实协议反序列化不可达；
- serde alias 只放宽输入键名接受面，不改变输出形态；
- 注册表键与执行器类型映射不变，用户配置与存量数据无感。
