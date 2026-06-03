# CLAUDE.md

## 项目概述
ntd (Nothing Todo) 是一个 AI Todo 应用，基于 Rust 后端 + React 前端，支持 Claude Code 和 JoinAI 执行器。

## 开发流程

**禁止直接在主分支 (main) 上写代码。所有代码改动必须先创建分支，在分支上完成开发后再通过 PR 合入 main。**

## 代码注释规范

**强制要求：所有新增/修改的代码必须带注释。**

- **逐行注释**：每一行代码都要写注释，解释「为什么这么写」（不是「写了什么」）。说明意图、设计取舍、边界条件、踩过的坑，而不是复述语法。
- **段落总览注释**：在大段代码（如函数实现、复杂逻辑块、状态机分支）之前，先用一段注释说明整体的处理思路、输入输出和关键步骤，让读者不用读代码就能理解做了什么。
- **避免无意义注释**：`// 自增 i` 这类复述代码本身的注释属于噪音，要写成「为什么需要自增」「自增的边界是什么」。
- **修改既有代码时**：如果改动了原有逻辑，要同步更新或新增注释，不能让注释与代码脱节。

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
```

### 端口区分
| 环境 | 端口 | 配置 | 数据库 |
|------|------|------|--------|
| 生产 | 8088 | config.yaml | data.db |
| 开发 | 18088 | config.dev.yaml | data.dev.db |

## 技术栈
- 后端: Rust (Axum框架)
- 前端: React + Vite + Ant Design
- 数据库: SQLite + SeaORM

## 目录结构
- `backend/` - Rust 后端代码
- `frontend/` - React 前端代码
- `tunnel.sh` - 内网穿透脚本

## 前端测试验证

**重要：修改前端 UI 后，必须使用 Playwright 进行自动化验证，再通知用户。**

### Playwright 测试脚本位置
测试脚本位于 `/tmp/` 目录下，文件名格式为 `check_*.js`

**运行方式**：由于 playwright 依赖在 `frontend/node_modules/` 中，需要在 `frontend/` 目录下执行：
```bash
cd frontend && npx playwright test --reporter=list
```

### 验证流程
1. 修改前端代码后，执行 `make dev` 重启开发服务
2. 使用 Playwright 编写测试脚本验证 UI 效果
3. 验证通过后再通知用户

### 常用验证脚本示例

```javascript
// 验证深色模式组件
const { chromium } = require('playwright');
(async () => {
  const browser = await chromium.launch();
  const context = await browser.newContext({ colorScheme: 'dark' });
  const page = await context.newPage();

  // 设置 localStorage 以触发 ThemeProvider 的深色模式
  await page.goto('http://localhost:18088');
  await page.evaluate(() => localStorage.setItem('app_theme', 'dark'));
  await page.reload();
  await page.waitForTimeout(2000);

  // 执行验证...
  const result = await page.evaluate(() => {
    const el = document.querySelector('.target-class');
    return { bg: el ? getComputedStyle(el).backgroundColor : null };
  });
  console.log('验证结果:', result);

  await page.screenshot({ path: '/tmp/verify.png' });
  await browser.close();
})();
```

### 内网穿透
如需远程验证，可使用 `tunnel.sh` 启动公网访问：
```bash
./tunnel.sh
```
