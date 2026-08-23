# 019-new指令未创建新session-缺陷说明

| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| AI (Pi) | 2026-08-09 | 初始版本 |
| AI (Claude) | 2026-08-23 | 按 Bug规范 L1 模板补全章节：触发条件结构化、行为偏差定义、影响范围、事实证据、不确定点、Gate 判定 |

## 1. Bug 基本信息（Identity）

- Bug ID：NTD-019
- 所属系统 / 服务：backend（feishu 卡片动作 / session 管理）
- 首次发现时间：2026-08-09（用户报告「/new 指令没有创建新 session」）
- 发现来源：用户报告 → 代码审查定位
- 当前状态：已确认存在 → 修复中（PR #1080，见修复总结）

## 2. Bug 是否被确认存在（Existence）

- [x] Bug 已被稳定复现（代码路径推演 + 字段语义实证）

### 2.1 复现环境

- 环境类型：生产 / 开发均存在
- 系统版本 / Commit ID：缺陷引入于 #884（`1862d158`，/help 卡片控制台上线引入卡片 /new 通路）；确认时为 2026-08-09 的 main
- 相关配置：飞书 bot 已配置工作空间与对话执行器（butler_executor 或默认 claudecode）

## 3. 触发条件（Trigger Conditions）

### 3.1 前置条件

- 系统状态：bot 在飞书**单聊**中发过带「新会话」按钮的卡片（如帮助控制台）
- 数据状态：`workspaces.executor_sessions` 中该 bot 所属 workspace 已存在 `dm:<executor>` 旧 session

### 3.2 输入条件

- 请求类型 / 参数：用户在**单聊（p2p）**会话点击卡片「新会话」按钮（card_callback 通路）
- 随后在该单聊发送下一条消息

### 3.3 时序 / 并发条件

- 无并发要求；仅需「点按钮 → 发消息」先后两次交互

## 4. 实际行为（Observed Behavior）

- 点按钮后收到正常提示「已开启新会话」
- 下一条消息仍复用旧 session 继续执行（延续历史上下文）
- 发生频率：单聊点卡片场景必现

## 5. 期望行为（Expected Behavior）

- 完全相同条件下：点「新会话」后，该单聊维度的旧 session 被清除
- 下一条消息以全新 session 执行（无历史上下文）
- 群聊维度的 session 不受单聊操作影响

## 6. 行为偏差定义（Deviation）

> 在【单聊点击卡片「新会话」按钮】下，系统实际执行了【保留 dm 维度旧 session，下一条消息继续 resume】，但期望执行的是【清除 dm 维度旧 session，下一条消息从新 session 开始】。

## 7. 影响范围（Impact）

- 受影响模块 / 功能：飞书卡片 `act:/new`（新会话按钮）；单聊对话执行器的 session 隔离
- 受影响用户或场景：所有在单聊使用卡片「新会话」按钮的用户
- 风险类型：数据错误（session 清错维度：单聊维度漏清）；无数据丢失、无安全风险、服务可用性不受影响
- 影响范围是否已知完全：是（文本 `/new` 与群聊卡片路径经核实不受影响，见第 8 节）

## 8. 已知边界与非问题（Non-Issues）

- 文本输入 `/new`（单聊或群聊）：行为正常（走 `from_chat_type` 正确推断 scope），不属于本 Bug
- 群聊中点卡片「新会话」按钮：行为符合预期，不属于本 Bug
- 群聊中「新会话后仍带旧上下文」类现象：属另一维度问题，不在本 Bug 范围内解决

## 9. 事实证据（Facts & Evidence）

- 用户报告原话：「/new 指令没有创建新 session」（2026-08-09）
- 复现路径：单聊点卡片「新会话」→ 提示成功 → 发送消息 → 消息仍带旧 session 上下文执行
- 代码事实（可验证）：
  - 卡片回调构造 `ChannelMessage` 时 `chat_type` 字段值为 `"card_callback"`（`backend/src/feishu/channel.rs`）
  - `act_new` 旧实现以 `channel.is_empty()` 推断会话维度（`backend/src/services/feishu_card_actions.rs`，#884 引入）
- 字段语义与 session 键位的完整证据链：见同目录缺陷分析 §2

## 10. 不确定点与待澄清事项（Uncertainties）

- 线上 card.action.trigger 回调 payload 的 `context` 是否实际携带 `chat_type` 字段：官方文档字段列表（url/preview_token/open_message_id/open_chat_id）未含该项，待真实 payload 抓包确认（影响修复方案 A 的实际有效性，不阻塞分析——方案 B 不依赖该字段）
- 除此之外无阻塞分析或修复的未决项

## 11. AI 阅读与处理约束（强制）

1. 仅基于本文档中的事实处理本 Bug，不补充隐含业务规则
2. 不将 Bug 解释为需求变更
3. 若 Bug 是否成立存在歧义，必须中止并请求澄清
4. 本文档不是修复指令

## 12. 进入下一阶段的判定（Gate）

- [x] Bug 描述完整，事实清晰，可进入分析阶段（已产出缺陷分析并完成修复，见修复总结）
