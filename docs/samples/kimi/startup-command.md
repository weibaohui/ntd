# Kimi 启动命令

## 命令格式

```bash
kimi --output-format stream-json -p <消息内容>
```

## 参数说明

| 参数 | 说明 |
|------|------|
| `--output-format stream-json` | 输出 JSONL 流式格式（每行一个 JSON 对象） |
| `-p` | 以 prompt 模式运行 |

## 会话恢复

```bash
kimi --output-format stream-json -r <session_id> -p <消息内容>
```

## 可执行文件路径

- 二进制名: `kimi`
