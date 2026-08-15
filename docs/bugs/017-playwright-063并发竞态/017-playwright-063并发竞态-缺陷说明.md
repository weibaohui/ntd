# NTD-017 Playwright 063 套件并发竞态

## 0. 变更记录

| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| AI (Claude) | 2026-08-15 | 初始版本 |

---

## 1. Bug 基本信息（Identity）

- Bug ID：`NTD-017`
- 标题：`063-task-pending-approval.spec.ts` 在并行 worker 下偶发全红
- 模块：`frontend/tests/063-task-pending-approval.spec.ts`（种子/清理/页面导航逻辑）
- 发现方式：全量 Playwright 套件回归（312 用例并发跑，063 稳定挂 3 项）
- 严重度：低（仅测试基础设施，不影响产品功能；但会掩盖真实回归信号）

---

## 2. 存在性（Existence）

### 2.1 现象

全量套件（多 worker 并发）下 `063-2/3/4/5` 随机失败：页面断言的待审批任务行/卡片
不渲染；单跑本文件 `--workers=1` 则 10/10 稳定通过。

### 2.2 复现路径

```bash
cd frontend && npx playwright test tests/063-task-pending-approval.spec.ts --workers=2 --repeat-each=2
# 失败样本：2 failed / 4 passed（失败面随机，多轮复跑失败用例不固定）
```

### 2.3 触发条件

- 同一 spec 文件被 ≥2 个 worker 并发执行（Playwright 默认并行）；
- 种子直落共享的 dev 库 `~/.ntd/data.dev.db`（该库同时被 18088 后端服务占用）。

---

## 3. 实际行为（Actual Behavior）

四层竞态叠加（修复时逐层剥离定位）：

1. **种子前缀互删**：种子/清理用固定前缀 `e2e-063-`，worker A 的 `afterAll cleanup()`
   按 `LIKE 'e2e-063-%'` 删掉 worker B 刚种入的行。
2. **database is locked**：sqlite3 CLI 写 dev 库与后端服务写事务撞锁，CLI 默认
   立即报错 `database is locked (5)` 而非等待。
3. **MAX(id) 反超**：种子 SQL 用 `(SELECT MAX(id) FROM loops)` 关联父子行，本 worker
   INSERT 与取 MAX 之间被对方 worker 的插入反超，A 的 task 挂到 B 的 loop 上，
   B 清理时连带删除 A 的数据。
4. **selected_workspace 被覆盖**（最深一层）：`goto→evaluate→reload` 导航模式下，
   evaluate 写入的 `selected_workspace=1` 被应用启动的异步 `SELECT_WORKSPACE`
   dispatch 覆盖回 `dirs[0]`（dev 库 project_directories 按 path 排序首项为 ws3）；
   种子落在 ws1、浏览器却请求 ws3，任务行随机消失。并发下 dispatch 与 evaluate
   的先后不确定，表现为偶发。

调试中另踩两个 sqlite3 CLI 坑（已在 spec 注释留档）：
- `PRAGMA busy_timeout=5000` 会向 stdout 输出一行 `5000`，污染 SELECT 回读
  （`Number("5000\n320")` → NaN，曾致 063-3/4/5 全挂）；
- dot-command（`.timeout`）混进位置参数会被当 SQL 静默吞掉整段脚本（退出码仍 0）。

---

## 4. 预期行为（Expected Behavior）

任意 worker 并发度下（含 `--workers=2 --repeat-each=2`），本套件 10 用例全部稳定通过；
worker 之间、worker 与后端服务之间对共享 dev 库的读写互不干扰。

---

## 5. 修复方案（Fix）

| 层 | 修复 |
|----|------|
| 1 | 种子前缀 run 唯一化：`e2e-063-w<TEST_PARALLEL_INDEX>-<时间戳>-<随机串>-`，清理只删自己的行 |
| 2 | sqlite3 会话加 busy_timeout：`sqlite3 -cmd '.timeout 5000' <db> <sql>`（dot-command 必须走 -cmd） |
| 3 | 关联 id 放弃 MAX(id)，改 run 唯一标题/名称精确匹配（`SELECT id FROM loops WHERE name='<唯一>'`） |
| 4 | `gotoTasks` 改 `addInitScript`：在任何应用代码执行前落 localStorage，`getInitialWorkspace` 直接读到目标 ws |

已知取舍：run 唯一前缀下，worker 被硬杀的残留行不再被下轮 cleanup 回收（旧固定前缀
可回收），dev 库崩溃场景会累积 `e2e-063-w*-...` 孤儿行——正确性无影响，量大时可手工清。

---

## 6. 验证（Verification）

- 并发复现 `--workers=2 --repeat-each=2`：连续 3 轮全 10/10 通过；
- 全量套件：312 passed / 0 failed / 18 skipped；
- 修复 PR：#1055。
