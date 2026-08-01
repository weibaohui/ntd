# 0. 文件修改记录表

| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| AI | 2026-08-01 | 初始版本：根因分析 |

# 1. 根因分析

## 1.1 代码变更链

| 版本 | 行为 |
|------|------|
| PR #918 之前 | `compose_review_prompt` 用 `{original_prompt}` 替换，`DEFAULT_REVIEWER_PROMPT` 也用 `{original_prompt}` |
| PR #945（feat 029, commit fbdce91b） | `compose_review_prompt` 改为 `{{original_prompt}}` 替换，`DEFAULT_REVIEWER_PROMPT` 常量同步改为双大括号 |
| **漏洞** | 迁移目录 0 处涉及 `review_templates.prompt` 的占位符替换；`ensure_default_review_template` 只在缺失时插入，**从不更新存量行** |

## 1.2 数据面

`review_templates` 表行 created_at=2026-07-27（旧版种入），prompt 中占位符为单大括号。PR #945 后新种入的行用双大括号。`ensure_default_review_template` 的 idempotent 逻辑是「有行就返回，无行才插入」，所以存量旧行永远不被更新。

## 1.3 替换逻辑

```rust
// 新代码只认双大括号
template_prompt
    .replace("{{original_prompt}}", &original.prompt)
    // ... 单大括号版本字面量残留
```

## 1.4 为什么不直接迁移 DB 数据

- 迁移需扫描 `review_templates` 表所有行，但用户可能自定义了模板，强制改写有风险
- 代码兼容更稳妥：先试双大括号、未命中再试单大括号，两种版本都正确替换
- 双大括号优先：新模板的 `{{original_prompt}}` 被替换后不会误伤单大括号版本

# 2. 修复策略

`compose_review_prompt` 中：先 `.replace("{{...}}", ...)` 再 `.replace("{...}", ...)`，同时覆盖新旧模板。

# 3. 测试覆盖

- `test_compose_double_braces_replaces_all`：新模板双大括号全替换
- `test_compose_single_braces_replaces_all`：存量模板单大括号全替换
- `test_compose_no_output_uses_default`：无输出用空串
- `test_compose_no_criteria_uses_fallback`：无验收标准用默认提示
- `test_compose_mixed_braces_both_replaced`：极端混用场景

# 4. 验证方式

跑评审实例，grep prompt 中 `{original_prompt}` 是否残留；检查 TC-E01/E06 用例。
