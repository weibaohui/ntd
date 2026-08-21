# NTD-019 zhanlu 执行器执行时未进入工作空间目录 — 缺陷说明

| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| Claude | 2026-08-18 | 初始版本（用户反馈：飞书消息跟执行器对话，执行器没有进入到工作空间的目录下） |

---

## 1. Bug 基本信息（Identity）

- Bug ID：`NTD-019`
- 所属系统 / 服务：`backend`（`backend/src/adapters/step_protocol.rs`、`backend/src/executor_service/spawn_lifecycle.rs`、`backend/src/services/executor_session.rs`）
- 首次发现时间：2026-08-18
- 发现来源：用户反馈（飞书消息与执行器对话时发现执行器不在工作空间目录）
- 当前状态：已确认存在

## 2. Bug 是否被确认存在（Existence）

- [x] Bug 已被稳定复现
- [ ] Bug 存在但不可稳定复现
- [ ] Bug 是否存在尚不确定（禁止进入 AI 分析阶段）

### 2.1 复现环境

- 环境类型：开发环境（端口 18088，`make dev`）；执行器为 zhanlu（`zl`）
- 系统版本 / Commit ID：`main`（ca72bc9e）
- 相关配置项：
  - 工作空间配置了有效 `path`（如 `/Users/mac/sticky-notes`）
  - 执行器配置为 `zhanlu`（事项的 executor 字段，或工作空间对话执行器 butler_executor）
- 复现方式：任一触发通路（飞书直连对话 / 事项执行）让 zhanlu 干活，让其汇报当前目录或读取工作空间内文件——不在工作空间目录内

## 3. 触发条件（Trigger Conditions）

### 3.1 前置条件

- 执行器选择 zhanlu（`zl` 二进制）
- 工作空间已配置路径

### 3.2 输入条件

- 以下任一通路触发 zhanlu 执行：
  - 事项（todo）执行：executor_service 主链路
  - 飞书单聊/群聊直连对话（dm_chat / butler_chat）
  - 黑板 Wiki 对话

## 4. 实际行为（Observed Behavior）

- ntd 把生效目录（workspace path，worktree 启用时为 worktree 路径）设置为子进程 cwd（`Command::current_dir`），但 zhanlu 的 argv 中**没有任何目录参数**：
  - `step_protocol.rs` `command_args`：`zl run --format json --dangerously-skip-permissions "<message>"`
  - `step_protocol.rs` `command_args_with_session`：`zl run --format json [-m <model>] [-s <sid>] --dangerously-skip-permissions "<message>"`
- `zl run` 的目录语义是「以 `--dir <path>` 参数为准」，不跟随进程 cwd，因此 zhanlu 实际工作目录不是工作空间目录。
- **发生频率**：必现（zhanlu 的每次执行）。

## 5. 期望行为（Expected Behavior）

- zhanlu 执行时通过 `zl run --dir <生效目录> ...` 显式接收目录（用户确认的 CLI 形态：`zl run --dir xxx`）。
- 生效目录与现有 cwd 语义一致：worktree 路径优先，回退 workspace path。
- 其余执行器（claudecode/codex/pi/kilo/opencode/mimo 等 10 家）行为不变——它们跟随进程 cwd，不注入目录参数。

## 6. 行为偏差定义（Deviation）

- ntd 已把目录信息写进进程 cwd，但 zhanlu 不读 cwd、只认 `--dir`，目录传递在「ntd → zl」边界断裂；表现为执行器在工作空间外的目录工作，对工作空间文件的读写、git 操作均不生效。

## 7. 影响范围（Impact）

- 影响面：所有使用 zhanlu 执行器的通路（事项执行 / 飞书直连对话 / Wiki 对话 / 环路步骤）。
- 严重级别：高（执行器完全不在目标目录工作，产出文件落错位置）。
- 不影响其余 10 个执行器（argv 不变）。

## 8. 已知边界与非问题（Non-Issues）

- 本机 dev 未安装 `zl` 二进制（仅残留 `~/.local/share/zhanlu` 数据目录），无法本地端到端复现；以 argv 代码事实 + 用户确认的 CLI 用法为判定依据。
- worktree 场景：`--dir` 应使用 worktree 路径（与 cwd 同源），不是缺陷但属于修复必须覆盖的边界。
- wiki 对话场景 cwd 是 wiki 目录（`~/.ntd/workspace/<id>/wiki`），`--dir` 跟随该目录，属既有设计而非偏差。

## 9. 事实证据（Facts & Evidence）

- `grep -rn -- '--dir' backend/src` 零命中——全部 11 个执行器适配器都不传目录参数。
- zhanlu argv 拼装唯一出处：`backend/src/adapters/step_protocol.rs:222`（`command_args`）与 `:235`（`command_args_with_session`），与 kilo/opencode/mimo 共用。
- 目录仅经进程 cwd 传递的两处 spawn 点：`backend/src/services/executor_session.rs:118`（直连对话/Wiki 对话）、`backend/src/executor_service/spawn_lifecycle.rs:216-218`（事项执行）。
- zl CLI 用法（用户确认）：`zl run --dir xxx`。

## 10. 不确定点与待澄清事项（Uncertainties）

- `--dir` 与 message 位置参数的先后顺序对 zl 解析器的影响未实测（修复采用「目录参数插在 message 之前」的保守顺序，见缺陷分析 §方案）。

## 11. AI 阅读与处理约束（强制）

- 只注入目录参数，不改动其余执行器的 argv 与 spawn 行为。
- 不改 `CodeExecutor` 既有方法签名（避免波及 11 个适配器）。
- 补齐单测：目录参数插入位置、flavor 差异（仅 Zhanlu）、flag/目录缺失时 argv 原样。

## 12. 进入下一阶段的判定（Gate）

- [x] Bug 描述完整，事实清晰，已进入分析 / 修复阶段
- [x] zhanlu argv 包含 `--dir <生效目录>`（插在 message 之前）
- [x] 其余执行器 argv 逐字节不变（单测锁定：`test_insert_dir_arg_执行器无目录flag时argv原样` 等 4 例）
- [x] `cargo clippy --all-targets -- -D warnings` 零告警；`cargo test` 1791 passed / 0 failed
