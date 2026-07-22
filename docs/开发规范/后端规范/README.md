# ntd 后端开发规范（Rust / Axum / SeaORM）

> 本文档定义 ntd 项目 Rust 后端开发的强制规范。
>
> **技术栈**：Rust + Axum + SeaORM + SQLite + tracing + cargo clippy（`-D warnings`）

## 目录

| 编号 | 文档 | 说明 |
|------|------|------|
| 01 | 总体原则 | 架构哲学与编码总则 |
| 02 | 项目分层 | Handler → Service → Repository → Model |
| 03 | Handler 规范 | 路由处理器编写规则 |
| 04 | Service 规范 | 业务逻辑层规范 |
| 05 | 错误处理规范 | 错误类型定义与传播 |
| 06 | Model 规范 | SeaORM Entity 定义规范 |
| 07 | 路由注册规范 | Axum 路由注册与中间件 |
| 08 | 数据库与 SeaORM 规范 | 查询、迁移、连接管理 |
| 09 | 事务规范 | 事务边界与一致性 |
| 10 | 测试规范 | 单元测试与集成测试 |
| 11 | 日志规范 | tracing / log 使用规约 |
| 12 | 配置规范 | 配置加载与环境区分 |
| 13 | 禁止清单 | 不允许出现的代码模式 |

> 所有规范与 `CLAUDE.md`、`AGENTS.md`、`docs/开发规范/AI协作开发约定.md` 共同生效。
