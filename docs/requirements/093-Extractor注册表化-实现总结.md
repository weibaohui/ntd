# 093-Extractor注册表化-实现总结

| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| AI (Pi) | 2026-08-08 | 初始版本 |
| AI (zhanlu) | 2026-08-10 | 按 CodeRabbit 评审同步文档与代码：executor_label 实为「删 match 留壳查 find_executor_by_type」，非整函数删除 |

> 对应设计：`docs/design/093-Extractor注册表化-设计.md`。093 专项 B4（重构批次 4），
> 消除 Repeated Switches / Shotgun Surgery：新增执行器从「改 5+ 处」变「注册表加一行」。

## 1. 实现了什么

| 变化 | 位置 |
|------|------|
| `ExecutorDef` 新增 `create_extractor: fn() -> Box<dyn EventExtractor>` 工厂字段 | `adapters/mod.rs` |
| `EXECUTORS` 13 条目全部挂载对应 Extractor 工厂（非捕获闭包→fn 指针入静态表） | `adapters/mod.rs` |
| 新增 `find_executor_by_type()` 注册表反查 | `adapters/mod.rs` |
| `EventPipeline::with_boxed_extractor()`（与泛型 `with_extractor` 并存） | `execution_events/pipeline.rs` |
| `create_pipeline_for_executor`：13 臂 match（~45 行）→ 查表 3 行 | `log_capture.rs` |
| `executor_label` 删除硬编码 match（与 `ExecutorDef.display_name` 逐字重复），**保留函数**作为注册表查询包装（调用点零改动） | `handlers/skills.rs` |

## 2. 与设计对应 / 关键实现点

- `models/mod.rs::ExecutorType::as_str` 按设计**保留**（枚举固有方法，是规范名单一事实源，非重复）；
- `executor_label` 函数体改查表（`find_executor_by_type`）+ `as_str()` 回退（比 panic 宽容，不返回空串）；函数壳保留，调用点零改动；
- 新增回归守卫测试 `test_every_executor_type_registered_with_factory`：13 个 ExecutorType 全部可查表且工厂可构造——「新增执行器忘注册/忘挂工厂」编译期之外的最后防线。

## 3. 测试与验证结果

- `cargo clippy --all-targets -- -D warnings`：零告警 ✅
- `cargo test --no-fail-fast`：1714 通过（唯一失败为预存量环境问题 git_sync）✅
- 假执行器端到端冒烟：注册表工厂构造的 pipeline 全链路正确（session_start→assistant→text→session_end 落库）✅

## 4. 收益与后续

- 新增第 14 个执行器：`EXECUTORS` 加一行（name + 类型 + 工厂同挂一处），不再触碰任何 match；
- 双解析体系彻底收敛的落点已就位：旧 `parse_output_line` 删除后，执行器↔解析关联只剩注册表。

## 5. 安全反思

纯构造路径重构；工厂产出实例与原 match 构造完全一致；无接口/行为变化。
