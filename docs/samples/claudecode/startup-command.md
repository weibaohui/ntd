# ClaudeCode 启动命令

## 命令格式

```bash
claude --dangerously-skip-permissions -p --output-format stream-json --verbose <消息内容>
```

## 参数说明

| 参数 | 说明 |
|------|------|
| `--dangerously-skip-permissions` | 跳过交互式权限确认 |
| `-p` | 以 prompt 模式运行（非交互） |
| `--output-format stream-json` | 输出 JSONL 流式格式（Claude Protocol） |
| `--verbose` | 详细输出 |

## 会话恢复

```bash
claude -p --output-format stream-json --verbose --session-id <session_id> <消息内容>
```

## 可执行文件路径

- 二进制名: `claude`
