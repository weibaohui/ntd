# CLAUDE.md

## 项目概述
ntd (Now Task, Done) 是一个 AI 驱动的任务引擎，基于 Rust 后端 + React 前端，支持 Claude Code 等执行器。

## 开发流程

**禁止直接在主分支 (main) 上写代码。所有代码改动必须先创建分支，在分支上完成开发后再通过 PR 合入 main。**

## AI 反思检查清单（代码编写前）

**强制要求：在写任何代码之前，AI 必须按顺序检查下列反思清单。**

- **YAGNI**：这东西真的需要存在吗？（不需要就跳过）
- **标准库**：标准库有吗？（有就用）
- **原生特性**：浏览器/平台原生支持吗？（有就用）
- **依赖**：已安装的包里有吗？（有就用）
- **极简**：能一行搞定吗？（一行搞定）
- **最后手段**：只写能跑通的最小代码

## 代码注释规范

**强制要求：所有新增/修改的代码必须带注释。**

- **逐行注释**：每一行代码都要写注释，解释「为什么这么写」（不是「写了什么」）。说明意图、设计取舍、边界条件、踩过的坑，而不是复述语法。
- **段落总览注释**：在大段代码（如函数实现、复杂逻辑块、状态机分支）之前，先用一段注释说明整体的处理思路、输入输出和关键步骤，让读者不用读代码就能理解做了什么。
- **避免无意义注释**：`// 自增 i` 这类复述代码本身的注释属于噪音，要写成「为什么需要自增」「自增的边界是什么」。
- **修改既有代码时**：如果改动了原有逻辑，要同步更新或新增注释，不能让注释与代码脱节。

## 代码质量规范

**强制要求：所有功能实现必须拆分为小函数，单函数控制在 30 行以内，且每个函数必须有单元测试并通过。**

### 函数长度限制

- 单个函数体（不含函数签名、空行、注释）不得超过 30 行。
- 超过 30 行的函数必须拆分为多个职责单一的子函数，并通过有意义的函数名表达其意图。
- 函数的圈复杂度应尽可能低：避免深层嵌套，超过两层嵌套应抽取为独立函数。
- **目的**：小函数易读、易测、易复用；函数名本身就是文档，读者无需深入实现细节即可理解逻辑。

### 单元测试要求

- **每一个公开函数/方法必须有对应的单元测试**。私有辅助函数如逻辑复杂也建议测试。
- 测试必须覆盖正常路径、边界条件、以及预期的错误处理分支。
- 测试命名规范：`test_<被测试函数名>_<场景描述>`，让失败时一眼看出哪个场景出问题。
- 运行方式：
  - Rust 后端：`cd backend && cargo test`
  - 如果后端有集成测试：`cd backend && cargo test --test integration_test`
- **所有测试必须在提交前通过**，不允许提交存在失败的测试。

### 编译告警清理

**强制要求：所有代码改动提交前，必须确保前后端均无编译告警/错误。**

- 后端 Rust：`cd backend && cargo clippy --all-targets -- -D warnings` 必须零告警零错误。
  - 生产代码禁止 `.unwrap()` / `.expect()` / `panic!`（`#[cfg(test)]` 和 `build.rs` 除外）。
  - 如因存量代码引入新告警，必须先在本 PR 中修复存量告警，再合入新代码。
- 前端 TypeScript：`cd frontend && npx tsc --noEmit` 必须零错误。
  - 前端构建：`cd frontend && npm run build`（包含 `tsc` + `vite build`）不应产生新告警。
- 新增 lint 或调整现有 lint 时，必须同步更新 `backend/Cargo.toml` 的 `[lints.*]` 和 `CONTRIBUTING.md` 的「Lint 策略」一节。
- **目的**：告警是潜在 bug 的信号，持续累积会降低代码可维护性；零告警策略让每次修改都有清晰的"红线"。

### 示例

❌ 反例（注释复述了代码，没解释为什么）：
```ts
// 调用 loadExecutionRecords
await loadExecutionRecords(1, historyLimit);
```

✅ 正例（注释解释了意图与取舍）：
```ts
// 执行成功后立即重新拉取列表，确保用户能看到刚创建的记录；
// 回到第 1 页是因为新记录按时间倒序排在最前面，停留在原页会看不到。
await loadExecutionRecords(1, historyLimit);
```

✅ 段落总览示例：
```ts
// 切换 Todo 时重新加载执行记录与汇总信息。
// 使用 cancelledRef 防御快速切换造成的竞态：晚返回的请求若发现已切换，直接丢弃结果。
// 依赖 historyLimit / historyStatusFilter，因为分页大小或筛选条件变化也要重拉。
useEffect(() => { ... }, [selectedTodoId, historyLimit, historyStatusFilter]);
```

## 生产环境 vs 开发环境

### 生产环境（端口 8088）
- 配置：`~/.ntd/config.yaml`
- 数据库：`~/.ntd/data.db`
- 日志：`~/.ntd/daemon.log`
- PID：`~/.ntd/daemon.pid`
- 管理命令：
```bash
ntd daemon install   # 安装为系统服务
ntd daemon start     # 启动
ntd daemon stop      # 停止
ntd daemon restart   # 重启
ntd daemon status    # 查看状态
```

### 开发环境（端口 18088）
- 配置：`~/.ntd/config.dev.yaml`（首次自动创建）
- 数据库：`~/.ntd/data.dev.db`
- 日志：`backend.dev.log`
- PID：`~/.ntd/dev.pid`
- 管理命令：
```bash
make dev    # 启动开发模式（构建前端 + 启动后端 embedded 模式）
make stop   # 停止开发实例
make build  # 构建生产版本
make clean  # 清理构建产物（frontend/dist + cargo target）
```

### 端口区分
| 环境 | 端口 | 配置 | 数据库 |
|------|------|------|--------|
| 生产 | 8088 | config.yaml | data.db |
| 开发 | 18088 | config.dev.yaml | data.dev.db |

## 技术栈
- 后端: Rust (Axum框架)，cargo clippy 强制执行 lint 策略
- 前端: React + TypeScript + Vite + Ant Design
- 数据库: SQLite + SeaORM
- 构建: Vite (前端)、Cargo (后端)、Make (项目级编排)

## 前端导入规范

**强制要求：在 `frontend/src` 目录内编写或修改前端代码时，跨目录导入统一使用 `@/` 绝对路径别名，不要使用 `../`、`../../` 这类相对路径回退。**

- 推荐写法：`import { useTheme } from '@/hooks/useTheme';`
- 禁止写法：`import { useTheme } from '../hooks/useTheme';`
- 适用范围：`frontend/src` 下的组件、hooks、utils、types、themes 等源码文件。
- 例外情况：同目录内的短相对导入可以保留，例如 `./constants`、`./helpers`。
- 修改旧代码时：如果顺手触达已有相对路径导入，优先一并改成 `@/`，保持项目风格一致。

## 目录结构
- `backend/` - Rust 后端代码（含 `backend/tests/` 集成测试）
- `frontend/` - React 前端代码（含 `frontend/tests/` Playwright 功能测试，详见「前端测试验证」一节）

## 前端测试验证

**重要：修改前端 UI 后，必须使用 Playwright 进行自动化验证，再通知用户。**

### 测试脚本位置（强制要求）

**所有使用 Playwright 编写的前端功能测试（含正式 spec 和调试脚本）必须统一放在 `frontend/tests/` 目录下，禁止散落到 `frontend/` 根目录、`/tmp/` 或其他位置。**

- 目录约定：与后端 `backend/tests/` 保持一致，前端对应 `frontend/tests/`。
- 文件命名：
  - 正式 spec：`frontend/tests/**/*.spec.ts`，由 `@playwright/test` 直接驱动。
  - 临时调试脚本：`frontend/tests/check_*.cjs` 或 `frontend/tests/check_*.js`，按需保留/清理。
- Playwright 配置：`frontend/playwright.config.ts` 必须将 `testDir` 指向 `frontend/tests`，并在 `testMatch` 中覆盖 spec 与调试脚本。
- 禁止放在 `/tmp/` 等系统临时目录：CI、他人复跑、回归对比都依赖仓库内可追溯的脚本。

### 当前实际情况

- 历史上曾把 spec（如 `frontend/e2e-test.spec.ts`）和调试脚本（`test_*.cjs`、`debug_click.cjs`、`inspect.cjs` 等）直接放在 `frontend/` 根目录，违反上述约定，需要在改动 UI 时顺手迁回 `frontend/tests/` 并同步更新 `playwright.config.ts`。
- `frontend/test-results/` 是 Playwright 产物目录，由运行自动生成，已在 `.gitignore` 中忽略（除错误上下文外的报告），不要手动提交。

### 运行方式

由于 Playwright 依赖位于 `frontend/node_modules/`，需要在 `frontend/` 目录下执行：

```bash
cd frontend && npx playwright test --reporter=list
```

针对单个调试脚本（仍位于 `frontend/tests/` 下）：

```bash
cd frontend && npx playwright test tests/check_xxx.spec.ts --reporter=list
```

### 验证流程

1. 修改前端代码后，执行 `make dev` 重启开发服务（默认监听 `http://localhost:18088`）。
2. 在 `frontend/tests/` 下编写或更新对应的 Playwright spec / 调试脚本。
3. 若新增或移动了 spec 文件，同步更新 `frontend/playwright.config.ts` 的 `testDir` / `testMatch`。
4. 运行 Playwright 验证 UI 效果；不通过则继续修复，直到用例稳定。
5. 验证通过、确保无遗留 `/tmp/` 散落脚本后再通知用户。

### 常用验证脚本示例

```javascript
// 文件位置：frontend/tests/check_theme.spec.ts
// 用途：验证深色模式组件渲染
import { test, expect } from '@playwright/test';

test('深色模式渲染校验', async ({ page }) => {
  // 启动无头浏览器，并预设 colorScheme 为 dark，
  // 让 ThemeProvider 初始化阶段直接进入暗色主题分支。
  const browser = await chromium.launch();
  const context = await browser.newContext({ colorScheme: 'dark' });
  const page = await context.newPage();

  // 通过 localStorage 写入主题键，刷新后由 ThemeProvider 接管，
  // 避免仅依赖系统色导致用例在 CI 上不稳定。
  await page.goto('http://localhost:18088');
  await page.evaluate(() => localStorage.setItem('app_theme', 'dark'));
  await page.reload();
  await page.waitForTimeout(2000);

  // 采集目标节点的实际样式，作为断言依据；
  // 这里以背景色为例，验证主题色板生效。
  const result = await page.evaluate(() => {
    const el = document.querySelector('.target-class');
    return { bg: el ? getComputedStyle(el).backgroundColor : null };
  });
  console.log('验证结果:', result);

  // 截图留档，便于在 PR 中附图说明。
  await page.screenshot({ path: 'frontend/tests/__screenshots__/verify.png' });
  await browser.close();
});
```

## 证据与截图发布规范

**强制要求：测试结论、功能完成证据的截图必须发布到 PR 评论或 GitHub Issue 中，不得作为源码提交到 git 仓库。**

### 发布位置

- **PR 评论**：使用 `gh pr comment <PR号> --body-file <说明文件>` 把 Markdown 结论发到 PR；如果有图，直接在 PR 页面的评论框拖拽上传。
- **Issue 评论**：在关联 issue 下用 `gh issue comment <Issue号> --body-file <说明文件>` 发布证据。

### 禁止行为

- ❌ **不要** 把截图（`.png`/`.jpg`/`.gif`/`.webp` 等）放进 `frontend/`、`backend/`、`docs/`、`tests/` 等源码目录后 `git add` 提交。
- ❌ **不要** 把验证截图放进 `frontend/tests/__screenshots__/` 后 `git add` —— 该目录在 `.gitignore` 中已忽略，目的是避免污染 git 历史。
- ❌ **不要** 把截图塞进 Git LFS 提交，除非项目本身已经启用 LFS 流程。
- ❌ **不要** 把 PNG/JPG 二进制文件 base64 编码后塞进 Markdown 文档随代码提交 —— 这同样会污染 diff。

### 允许的视觉证据形式

- ✅ 纯文本 / Markdown 表格、ASCII 流程图、代码块。
- ✅ SVG 矢量图（文本形式，diff 友好）。
- ✅ 外部链接：Figma 设计稿、在线仪表盘截图链接、CI 构建产物的 URL。
- ✅ Markdown 中的相对路径图片引用（指向 PR 评论或 issue 中已上传的图片 URL）。

### 为什么这么要求

- 仓库是**源代码版本库**：二进制截图会让 `git clone` 变慢、history 臃肿、code review 噪声大。
- **可读性**：截图的权威来源是 PR/Issue 评论流，读者在 PR 页面直接可见，无需 checkout 仓库。
- **一致性**：Playwright 的 `test-results/`、截图目录已在 `.gitignore` 中忽略；保持这条边界，不要因为"留个证据"就破例提交。
- **可追溯**：图片上传到 GitHub 后会得到永久 URL，git 仓库的 commit 历史与 PR/Issue 评论流是独立通道，互不污染。

### 常用发布命令

```bash
# 把准备好的证据 Markdown 发到 PR 评论
gh pr comment 650 --body-file /tmp/evidence.md

# 发到关联 issue 评论
gh issue comment 647 --body-file /tmp/evidence.md

# 查看自己的 PR 列表
gh pr list --author @me
```

