# 故障排查

ntd 常见问题的快速诊断指南。

## 1. 服务起不来

### 1.1 端口被占用

```bash
lsof -i :8088
# 或
lsof -i :18088
```

杀掉占用的进程或改 `server_port`。

### 1.2 配置文件错误

```bash
# 测试 YAML 语法
python3 -c "import yaml; yaml.safe_load(open('~/.ntd/config.yaml'))"
```

### 1.3 数据库锁

```bash
# 找持锁进程
lsof ~/.ntd/data.db
```

杀掉其他 sqlite3 进程或 DB Browser 等 GUI 工具。

## 2. Todo 跑不起来

### 2.1 执行器没配

去「执行器管理」配至少一个，参考 [executors.md](../settings/executors.md)

### 2.2 执行器检测失败

- 检查 binary_path 是不是真的可执行
- `which claudecode` 看 PATH 是否对
- macOS 首次运行要「系统设置 → 隐私与安全」允许

### 2.3 Token 限额

执行器返回 401/429 → 检查 API key / 配额。

## 3. WebSocket 频繁断连

### 3.1 反向代理超时

nginx 默认 60s。改成：

```nginx
proxy_read_timeout 3600s;
```

### 3.2 浏览器问题

- 隐身模式试
- 关掉浏览器扩展

## 4. 云端同步问题

详见 [cloud-sync.md 第 8 节](../settings/cloud-sync.md#8-故障排查)

| 现象 | 排查 |
|------|------|
| 一直转圈最后超时 | `curl server_url/health` 看连通性 |
| Token 无效 | 去 ntd-cloud 重新签发 |
| 拉取 0 条 | 改 overwrite 策略或先看云端有没有数据 |
| 推送 200 但云端没收到 | 看后端日志 cloud 关键字 |

## 5. 飞书 Bot 无反应

详见 [messages-feishu.md 第 6 节](../settings/messages-feishu.md#6-故障排查)

| 现象 | 排查 |
|------|------|
| 收不到消息 | 群白名单 / Bot 状态 / 飞书后台权限 |
| 推送失败 | 推送开关 / target 配置 |
| 历史为空 | fetcher 没启动 / 消息太老被清 |

## 6. 性能问题

### 6.1 Dashboard 加载慢

- execution_records 太多
- 解决：删老记录 + VACUUM

### 6.2 Todo 列表卡

- 软删的 Todo 太多（数据库扫描成本）
- 物理删除：`DELETE FROM todos WHERE deleted_at IS NOT NULL`

### 6.3 内存占用高

- 长时间运行（> 1 周）可能积累
- 重启服务

## 7. 升级后问题

### 7.1 前端样式错乱

- 浏览器缓存老 dist 文件
- 硬刷新：Cmd+Shift+R / Ctrl+Shift+R
- 或无痕模式

### 7.2 API 行为变了

- 看 release notes
- 配置文件可能新增必填字段
- 用 `ntd --version` 确认升级成功

## 8. 日志在哪

| 模式 | 位置 |
|------|------|
| 生产 daemon | `~/.ntd/daemon.log` |
| 开发模式 | `backend.dev.log`（仓库根） |

## 9. 抓取日志给开发者

如果要找 bug：

```bash
# 收集最近 1000 行
tail -n 1000 ~/.ntd/daemon.log > /tmp/ntd-bug.log

# 加 system info
echo "---" >> /tmp/ntd-bug.log
echo "ntd version: $(ntd --version)" >> /tmp/ntd-bug.log
echo "OS: $(uname -a)" >> /tmp/ntd-bug.log
echo "Mode: $(echo $NTD_MODE)" >> /tmp/ntd-bug.log
```

## 10. 重置 ntd（最后手段）

⚠️ **会丢失所有数据**，慎用！

```bash
ntd daemon stop
rm -rf ~/.ntd
ntd daemon install
ntd daemon start
```

之前的所有 Todo、配置、备份都没了。**确认有异地备份再操作**。
