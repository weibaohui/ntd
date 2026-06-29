# Kimi 启动命令

## 命令格式

```bash
kimi --print --output-format stream-json -p <消息内容>
```

## 参数说明

| 参数 | 说明 |
|------|------|
| `--print` | 打印模式（非交互） |
| `--output-format stream-json` | 输出 JSONL 流式格式 |
| `-p` | 以 prompt 模式运行 |

## 会话恢复

```bash
kimi --print --output-format stream-json -p -S <session_id> <消息内容>
```

## 可执行文件路径

- 二进制名: `kimi`
