# 012-kilo系执行器非零退出码被误判失败-缺陷说明

| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| AI (Pi) | 2026-08-08 | 初始版本 |

## 1. Bug 基本信息

- Bug ID：NTD-012
- 所属系统：backend（executor_service / adapters）
- 首次发现时间：2026-08-08
- 发现来源：代码审查（093 优化扫描专项第 5 项「执行器双解析体系」盘点过程中）
- 当前状态：已确认存在

## 2. Bug 是否被确认存在

- [x] Bug 已被稳定复现（代码路径推演 + 单元级复现测试）

### 2.1 复现环境

- 环境类型：生产 / 开发均存在
- Commit：main @ 67c5d924（091 性能优化合入后）
- 相关配置：使用 kilo / mimo / opencode / zhanlu 四个执行器之一

## 3. 触发条件（最小条件集合）

1. 任务使用 kilo / mimo / opencode / zhanlu 执行器；
2. 执行器正常输出 JSONL（`step_finish` 事件被新 EventPipeline 成功解析——**这是 pipeline 引入后的恒成立条件**）；
3. 进程以非零退出码结束（kilo 已知会返回 144 等；mimo 文档记载模型超时也会非零退出）。

## 4. 现象（What）

- **期望**：进程非零退出但收到了 `step_finish` 事件 → 任务判定为**成功**（4 个执行器 `check_success` 的明确设计语义：「非零退出码但有 finish 事件就算成功」）。
- **实际**：任务被判定为**失败**。

## 5. 影响范围

- 受影响的执行器：kilo、mimo、opencode、zhanlu（4 个共享同一协议族的执行器）。
- 不受影响：claude_code、codex、hermes、kimi、pi、atomcode、codebuddy、codewhale、mobilecoder（`check_success` 不依赖流式副作用状态）。
- 用户可感知后果：任务实际完成但状态被标记失败，可能触发不必要的自动返工/重试逻辑。

## 6. 边界（何时不成立）

- 退出码为 0 时不受影响（`check_success` 首行直接返回 true）。
- 进程被 kill / 无 step_finish 事件的真实失败场景不受影响（误判方向是把成功判成失败，不是反过来）。
