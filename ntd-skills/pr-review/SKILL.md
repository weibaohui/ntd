---
name: pr-review
description: "PR/MR 代码评审方法论 — 转化自 Claude Code 官方 code-review 插件与 pr-review-toolkit 插件。六视角专项评审（规范符合/测试覆盖/注释准确性/静默失败/类型设计/代码简化）+ 置信度评分过滤误报 + 结构化报告。Keywords: review, code review, pr review, mr review, pull request, merge request, 评审, 审查, code-reviewer, pr-test-analyzer, comment-analyzer, silent-failure-hunter, type-design-analyzer, code-simplifier, confidence score, false positive, 置信度, 误报, diff review, pre-merge check. Triggers on: review 这个 PR, 评审, 代码评审, code review, review my pr, 审查代码, 帮我看看这个 PR, 合并前检查, pre-merge review, PR 检查, MR 评审, 审查这个分支, 看看改动有没有问题"
version: "1.0.0"
license: MIT
executors: [claudecode, atomcode, mobilecoder, hermes, codex, codebuddy, opencode, kimi, pi, agents]
metadata:
  author: weibaohui
  tags: [code-review, pull-request, quality-gate, 评审, 代码审查]
  origin: "Converted from Claude Code official plugins: code-review + pr-review-toolkit (github.com/anthropics/claude-plugins-official)"
---

# PR 评审（pr-review）

> 转化自 Claude Code 官方插件 `code-review`（多 agent 编排 + 置信度评分）与
> `pr-review-toolkit`（六视角专项清单），融合为单一可执行方法论。
> 工具无关：任何能读 diff、跑命令的 AI 都可执行。

## 一、何时使用

- 用户要求评审 PR / MR / 分支改动（"review 这个 PR"、"看看改动有没有问题"）
- 自己刚完成一块较大改动，声明完成前的自查
- 合入 main 前的质量门禁

## 二、评审工作流（六步）

```
① 定范围 → ② 资格预检 → ③ 六视角评审 → ④ 逐条置信度评分 → ⑤ 过滤误报 → ⑥ 结构化报告
```

### ① 定范围

- 拿到 diff：`git diff main...HEAD`（或 PR 页面 diff），列出变更文件清单
- 收集项目规范文件：根 `CLAUDE.md` / `AGENTS.md` / 各目录局部规范
- 判断涉及面（决定激活哪些视角）：
  - 有逻辑/功能改动 → code-reviewer（**永远激活**）
  - 有测试文件或新逻辑 → pr-test-analyzer
  - 有注释/文档改动 → comment-analyzer
  - 有错误处理/catch/fallback → silent-failure-hunter
  - 有新类型/类型修改 → type-design-analyzer
  - 全部通过后 → code-simplifier（润色建议）

### ② 资格预检

命中任一条件则**停止评审**并说明原因：

- PR 已关闭 / 合并 / draft
- 纯自动化产物（lock 文件更新、生成代码、版本号 bump）
- 本 AI 已评审过同一 commit 且无新变化

### ③ 六视角评审（各自独立视角，发现互不去重）

按「四、视角清单」逐视角过 diff。每条发现记录：文件:行号、描述、依据（规范条款/bug 机理）。

**关键纪律**：只评审 diff 本身和必要的直接上下文；不跑构建/测试（CI 的职责）；
lint/类型错误默认 CI 会抓，不占用评审篇幅。

### ④ 逐条置信度评分（0-100）

对每条发现独立打分（rubric 照抄 code-review 插件）：

| 分数 | 含义 |
|------|------|
| 0 | 完全不置信：经不起推敲的误报，或存量问题 |
| 25 | 有点置信：可能是真问题也可能是误报；无法验证为真；风格类且规范未明确要求 |
| 50 | 中等置信：验证为真，但属 nitpick 或实践中很少触发；相对整个 PR 不重要 |
| 75 | 高度置信：复核过，实践中很可能踩到；现有实现确实不足；或规范明确点名的问题 |
| 100 | 绝对确定：复核确认必然发生且高频；证据直接确凿 |

规范符合类问题：必须**二次确认规范文件确实明确点名了该问题**，否则上限 25。

### ⑤ 过滤

- **丢弃所有 < 80 分的发现**（宁缺毋滥，误报比漏报更伤信任）
- 剩余按分数分级：Critical（90-100）/ Important（80-89）

### ⑥ 结构化报告

按「五、输出模板」输出。每条必须：文件:行号 + 问题描述 + 依据 + 具体修法建议。
引用代码必须给可点击的永久链接（完整 sha + 行区间，如 `https://github.com/<org>/<repo>/blob/<full-sha>/<path>#L10-L15`）。

## 三、误报清单（这些必须过滤掉）

- 存量问题（非本 PR 引入）
- 看起来像 bug 但验证后不是
- 高级工程师不会点名的学究式 nitpick
- linter/类型检查/编译器能抓到的问题（导入缺失、类型错误、格式）
- 泛泛的"代码质量"意见（缺测试、泛安全、泛文档），除非规范明确要求
- 规范点名但代码里已显式豁免（如 `#[allow]` 且附理由）
- 大概率是有意的行为变更，且与 PR 主题直接相关
- 位于用户未修改的行上的问题

## 四、六视角清单

### 1️⃣ code-reviewer（规范符合 + bug 侦测）— 永远激活

- **规范符合**：对照项目 CLAUDE.md/AGENTS.md 的显式规则（导入模式、框架约定、
  注释规范、函数长度、测试要求、禁止清单）
- **bug 侦测**：逻辑错误、null/None 处理、竞态/TOCTOU、资源泄漏、安全问题、
  性能问题（N+1、无界查询、重复计算）
- 只报会真实影响功能的问题

### 2️⃣ pr-test-analyzer（测试覆盖）

- 行为覆盖而非行覆盖：关键路径、边界条件、错误分支是否有用例
- 优先找：**未测的错误处理路径**（静默失败温床）、缺负数用例、缺并发/异步用例
- 每条建议标注：防什么回归 + 关键度 1-10（8+ 才进报告）
- 测试质量问题：测实现而非测行为、过度耦合内部细节

### 3️⃣ comment-analyzer（注释准确性）

- **事实核对**：注释的每个论断与代码实际行为一致（参数、返回值、边界、性能声明）
- 注释与代码**直接矛盾**（最优先报告，如"直接 await"但代码是 spawn）
- 重复/残留注释（编辑遗留的新旧两版并存）
- 注释引用已重构掉的代码、过时假设
- 只复述代码的噪声注释（应删）；解释"为什么"的注释（应留）

### 4️⃣ silent-failure-hunter（静默失败）

- 空 catch、只 log 就继续、错误返默认值无日志、`.ok()`/`.unwrap_or_*` 吞错
- catch 过宽可能误伤无关错误（列出可能被隐藏的具体错误类型）
- fallback 未告知用户（用户分不清"真空"与"故障降级"）
- 应传播却被就地吞掉的错误
- 用户-facing 错误信息是否可操作（说了什么错 + 怎么办）
- 例外：有注释说明理由的故意降级（如"增强数据缺失可接受"）不算问题，但缺日志的要点名

### 5️⃣ type-design-analyzer（类型设计）

- 只评**新增/修改的类型**，四轴打分（1-10）：封装性 / 不变量表达 / 不变量效用 / 不变量强制
- 红旗：贫血模型、暴露可变内部、不变量只靠注释、构造期不校验、
  部分字段语义互斥但用裸字段表达（应用 enum）

### 6️⃣ code-simplifier（简化润色）— 前面视角通过后再做

- 减少嵌套、消除冗余抽象、合并重复逻辑
- 可证明的死代码（如对已确定失败的输入重复同一操作）
- 嵌套三元 → if/else；清晰优先于行数
- **不改行为**；过度简化（损害可读性换行数）反而是问题

## 五、输出模板

```markdown
# PR Review Summary — <PR 标题>

## Critical Issues (N found)
- [<视角>] <问题描述> [<file>:<line>]
  依据：<规范条款/bug 机理>；修法：<具体建议>

## Important Issues (N found)
- [<视角>] ...

## Suggestions (N found)
- [<视角>] ...

## Strengths
- <值得肯定、应被保持的做法>

## 行动建议
<按优先级排序的下一步>
```

无 ≥80 分问题时：
```markdown
### Code review
No issues found. Checked for bugs and <规范文件> compliance.
```

## 六、评审后处理

- 发现被修复后：复核 diff，确认每条问题已解决或标注跳过原因
- 机器人评审意见（CodeRabbit 等）同样适用本方法论：**逐条复核置信度**，
  已修复的注明修复 commit，真问题按级修复，误报给跳过理由
- 评审与处理记录发到 PR 评论留痕（有 gh 时：`gh pr comment`）

## 七、与 ntd 项目规范的对接

在本仓库执行评审时，以下项目红线是 code-reviewer 视角的必查项：

- 生产代码 `.unwrap()` / `.expect()` / `panic!`（`#[cfg(test)]` 除外）
- 单函数体 >30 行（豁免场景见 CLAUDE.md 四种）
- 逐行"为什么"注释 + 段落总览注释；注释与代码脱节
- 后端 `cargo clippy --all-targets -- -D warnings`、前端 `npx tsc --noEmit`
- 前端跨目录用 `@/` 绝对路径
- Handler 不直接操作 SQL 拼接用户输入；SQL 参数绑定
