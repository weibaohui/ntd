# Pi 启动命令

## 命令格式

```bash
pi -p --mode json <消息内容>
```

## 参数说明

| 参数 | 说明 |
|------|------|
| `-p` | 以 prompt 模式运行（非交互） |
| `--mode json` | 输出 JSONL 格式 |

## 注意事项

Pi 启动时会先通过 stdin 询问确认，需要输入 `y` 来确认。

## 会话恢复

```bash
pi -p --mode json --session <session_id> <消息内容>
```

## 可执行文件路径

- 配置路径: `~/.pi/pi`（非标准路径）
- 二进制名: `pi`
