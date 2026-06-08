# AGENTS.md

## 项目概述
ntd (Nothing Todo) 是一个 AI Todo 应用，基于 Rust 后端 + React 前端，支持 Claude Code 和 JoinAI 执行器。

## 开发流程

**禁止直接在主分支 (main) 上写代码。所有代码改动必须先创建分支，在分支上完成开发后再通过 PR 合入 main。**

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

## 前端导入规范

**强制要求：在 `frontend/src` 目录内编写或修改前端代码时，跨目录导入统一使用 `@/` 绝对路径别名，不要使用 `../`、`../../` 这类相对路径回退。**

- 推荐写法：`import { useTheme } from '@/hooks/useTheme';`
- 禁止写法：`import { useTheme } from '../hooks/useTheme';`
- 适用范围：`frontend/src` 下的组件、hooks、utils、types、themes 等源码文件。
- 例外情况：同目录内的短相对导入可以保留，例如 `./constants`、`./helpers`。
- 修改旧代码时：如果顺手触达已有相对路径导入，优先一并改成 `@/`，保持项目风格一致。

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
