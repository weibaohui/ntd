# 快速开始 - 安装

## 一键安装（推荐）

```bash
npm install -g @weibaohui/nothing-todo
ntd daemon install   # 注册为系统服务（开机自启）
ntd daemon start     # 立即启动
```

打开浏览器访问 http://localhost:8088

## 服务管理

```bash
ntd daemon install    # 注册为系统服务
ntd daemon start      # 启动
ntd daemon stop       # 停止
ntd daemon restart    # 重启
ntd daemon status     # 查看状态
```

## 配置文件位置

安装后第一次启动会自动创建：

| 文件 | 路径 |
|------|------|
| 配置 | `~/.ntd/config.yaml` |
| 数据库 | `~/.ntd/data.db` |
| 日志 | `~/.ntd/daemon.log` |
| PID | `~/.ntd/daemon.pid` |
| 备份目录 | `~/.ntd/backups/` |

## 卸载

```bash
ntd daemon stop
npm uninstall -g @weibaohui/nothing-todo
# 数据保留在 ~/.ntd/ 下，想全删：rm -rf ~/.ntd
```

## 下一步

- [首次运行](first-run.md) — 配置执行器、创建第一个 Todo
- [开发 vs 生产](dev-vs-prod.md) — 端口 8088 和 18088 区别
- [执行器管理](../settings/executors.md) — 必须先配好至少一个执行器
