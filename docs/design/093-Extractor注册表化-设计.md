# 093-Extractor注册表化-设计

| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| AI (Pi) | 2026-08-08 | 初始版本 |
| AI (zhanlu) | 2026-08-10 | 按 CodeRabbit 评审同步文档与代码：executor_label 实为「删 match 留壳查 find_executor_by_type」，非整函数删除 |

> 093 专项 B4（重构批次 4）。源自 skill 扫描诊断第 4 条：Repeated Switches + Shotgun Surgery——
> `ExecutorType` 大 match 散落多处，新增执行器要同步修改 N 个文件。

## 1. 现状与证据

| 位置 | match 职责 | 处置 |
|------|-----------|------|
| `log_capture.rs::create_pipeline_for_executor` | 13 臂 match 构造对应 Extractor | **本 PR 收敛进注册表** |
| `handlers/skills.rs::executor_label` | 13 臂 match 返回显示名 | **纯重复**（`ExecutorDef.display_name` 逐字相同），本 PR 删除改查表 |
| `models/mod.rs::ExecutorType::as_str` | 枚举自身的规范名映射 | **保留**——这是枚举的固有方法，是单一事实源而非重复 |

项目已有 `ExecutorDef` 静态注册表（`EXECUTORS: &[ExecutorDef]`，含 name/executor_type/binary_name/display_name/session_dir/aliases）——注册点早就存在，只是没挂 Extractor。

## 2. 设计

### C1：`ExecutorDef` 挂提取器工厂

```rust
pub struct ExecutorDef {
    ...
    /// 093-B4：该执行器的事件提取器工厂（双解析体系收敛的注册点）。
    /// 非捕获闭包强制转换为 fn 指针，可放入静态表。
    pub create_extractor: fn() -> Box<dyn EventExtractor>,
}
```

`EXECUTORS` 13 个条目各挂 `|| Box::new(XxxExtractor::new())`。

### C2：注册表查询 + pipeline 构造塌缩

```rust
/// 按 ExecutorType 反查注册项（registry 反向索引）
pub fn find_executor_by_type(et: ExecutorType) -> Option<&'static ExecutorDef>
```

`create_pipeline_for_executor` 从 13 臂 match（~45 行）塌缩为：

```rust
let def = find_executor_by_type(executor.executor_type())?;
Some(EventPipeline::with_boxed_extractor((def.create_extractor)()))
```

`EventPipeline` 新增 `with_boxed_extractor(Box<dyn EventExtractor>)` 构造（与既有 `with_extractor(impl)` 并存——注册表只能返回 trait 对象）。

### C3：`executor_label` 删除硬编码 match，保留函数作为注册表查询包装

`skills.rs` 的 `executor_label(et)` 原是与 `ExecutorDef.display_name` 逐字重复的 13 臂 match。
实施时**保留函数壳**（调用点零改动），函数体改查注册表：
`crate::adapters::find_executor_by_type(et).map(|d| d.display_name).unwrap_or_else(|| et.as_str())`
（找不到时回退规范名，比 panic 宽容）；其测试同步改为断言注册表覆盖
（保留「新增执行器忘注册」的回归守卫语义）。

> 注（CodeRabbit #1010 评审）：初版本节写「删除 executor_label」且示例误用
> `find_executor(et.as_str())`——按名字符串反查对枚举语义不对（别名/大小写歧义），
> 实际落地为按 ExecutorType 反查 `find_executor_by_type`，特此更正。

## 3. 影响模块

`adapters/mod.rs`（ExecutorDef+EXECUTORS+find_executor_by_type）、`execution_events/pipeline.rs`（+with_boxed_extractor）、`executor_service/log_capture.rs`（match 塌缩）、`handlers/skills.rs`（删 executor_label+测试改断言）。

## 4. 收益与后续

- 新增第 14 个执行器：从「改 5+ 处」变「EXECUTORS 加一行」；
- 为双解析体系收敛铺路：旧 `parse_output_line` 删除后，执行器与解析的关联点只剩注册表一处。

## 5. 验证方案

1. 既有 log_capture / execution_events / skills 测试全绿；
2. 新增断言：13 个 ExecutorType 全部能在注册表查到且工厂可构造（防漏挂）；
3. clippy 零告警；假执行器冒烟（沿用 B2/B3 方法）。

## 6. 安全反思

纯构造路径重构；工厂返回的 Extractor 实例与原 match 构造完全一致；无接口/行为变化。
