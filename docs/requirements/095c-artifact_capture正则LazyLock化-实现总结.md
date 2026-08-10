# 095c-artifact_capture 正则 LazyLock 化-实现总结

| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| AI (zhanlu) | 2026-08-10 | 初始版本 |
| AI (zhanlu) | 2026-08-11 | 按 CodeRabbit 评审补记 LazyLock 初始化时机与 unwrap 失败语义变化 |

> 093 专项性能类第 3 项（清单标注 10 分钟级）：`artifact_capture.rs:301` 每次调用现编译正则。
> 改动微小，设计与总结合并为一份。

## 1. 问题与方案

`extract_first_url`（artifact 提取路径，loop step 产出提取时调用）每次调用都
`Regex::new(...)` 现编译。改为 `LazyLock<Regex>` 进程级一次编译，复用项目既有范式
（`adapters/mod.rs::THINK_RE`）。

顺带核实：同被扫描出的 `adapters/mod.rs:210`（think 标签正则）已是 LazyLock 范式；
`models/mod.rs:1390` 在测试代码内——两处均无需处理，生产现编译点仅此一处。

## 2. 变化点

| 文件 | 变化 |
|------|------|
| `services/process/artifact_capture.rs` | `extract_first_url` 改用 `static URL_RE: LazyLock<Regex>`；原 `.ok()?` 失败返回 None 改为 unwrap（见下方语义变化说明） |

## 3. 验证

- `artifact_capture` 模块 12 个既有测试全过（含 `test_extract_first_url` 行为回归）
- 改动文件 clippy 零告警

## 4. 语义变化说明（CodeRabbit #1014 评审后补记）

- **初始化时机**：LazyLock 的编译发生在**首次访问** `URL_RE` 时（非编译期/进程启动期），
  首次调用 extract_first_url 多付一次性编译成本，后续调用零成本；
- **失败语义**：旧 `.ok()?` 在正则编译失败时返回 None；新实现 unwrap 在失败时 panic。
  该变化可接受：模式是编译期固定字符串，编译失败只在正则引擎行为变化时发生
  （此时进程不可用，panic 比静默返回 None 更诚实）；与 `adapters/mod.rs::THINK_RE`
  的既有决策同款。

## 5. 安全反思

无输入面/权限/协议变化；正则模式不变，匹配行为逐字节一致。
