# 0. 文件修改记录表

| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| AI | 2026-08-01 | 初始版本：根因分析 |

# 1. 根因分析

`handlers/tasks.rs:55`：
```rust
let title = if title.len() > 60 { format!("{}…", &title[..60]) } else { title.to_string() };
```

- `title.len()` 返回字节数，`&title[..60]` 按字节切片
- CJK 字符占 3 字节，当第 58-60 字节落在字符中间时，str 切片越界 panic
- 语义上应限制 60 **字符**而非 60 字节，否则对拉丁文也偏短、对 CJK 直接崩溃

# 2. 修复策略

```rust
let title = if title.chars().count() > 60 {
    let truncated: String = title.chars().take(60).collect();
    format!("{}…", truncated)
} else { title.to_string() };
```
`chars()` 迭代 Unicode 标量值，`take(60)` 保证在字符边界截断。
