# Opencode 启动命令

## 命令格式

```bash
opencode run --format json --dangerously-skip-permissions <消息内容>
```

## 参数说明

| 参数 | 说明 |
|------|------|
| `run` | 运行子命令 |
| `--format json` | 输出 JSONL 格式 |
| `--dangerously-skip-permissions` | 跳过交互式权限确认 |

## 会话恢复

```bash
opencode run --format json -s <session_id> <消息内容>
```

## 可执行文件路径

- 二进制名: `opencode`
