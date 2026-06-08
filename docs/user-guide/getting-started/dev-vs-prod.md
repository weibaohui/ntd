# 开发与生产环境

ntd 区分**生产**和**开发**两套环境，用端口隔开避免冲突。

## 端口对照

| 环境 | 端口 | 配置文件 | 数据库 | 启动命令 |
|------|------|----------|--------|----------|
| 生产 | 8088 | `~/.ntd/config.yaml` | `~/.ntd/data.db` | `ntd daemon start` |
| 开发 | 18088 | `~/.ntd/config.dev.yaml` | `~/.ntd/data.dev.db` | `make dev` |

## 什么时候用哪个

- **生产** (`ntd daemon install`)：日常使用，开机自启，绑 8088
- **开发** (`make dev`)：改代码、调前端、跑新功能时用，绑 18088
- 两个环境**可以同时跑**，互不干扰

## 开发模式额外行为

- 构建前端（`npm run build` 一次性构建，**不**监听文件变化 — 改前端后需重新 `make dev`）
- 后端 `cargo run`（**不**自动重启 — 改 Rust 代码需 `Ctrl+C` 后再 `make dev`）
- 日志输出到 `backend.dev.log`
- CORS 允许任意 Origin（生产只允许同源）
- HTTP 请求体上限 10MB（默认）

## 开发常用命令

```bash
make dev    # 启动开发模式（build 前端 + cargo run 后端）
make stop   # 停开发实例
make build  # 生产构建
```

## 配置文件示例

`~/.ntd/config.dev.yaml`：

```yaml
server:
  host: 0.0.0.0
  port: 18088
database:
  url: sqlite:////Users/me/.ntd/data.dev.db
log:
  level: debug   # 开发模式默认 debug，看更详细日志
```

## 故障排查

### 端口占用

```bash
lsof -i :18088
# 或
lsof -i :8088
```

杀进程：`kill -9 <PID>`

### 配置写错

启动会读 YAML 校验。YAML 错误会直接 panic 启动失败，看 `~/.ntd/run.log`（macOS）/`journalctl --user -u ntd`（Linux）或 `backend.dev.log`。

### 数据库锁

SQLite 写锁。如果有别的进程打开了同一个 db 文件（ntd 自己、或 DB Browser），写会失败。

解决：关掉其他工具，或换一个数据库路径。
