# 0. 文件修改记录表

| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| AI | 2026-08-01 | 初始版本：BUG-002 缺陷说明 |

# 1. 缺陷标题

存量评审模板占位符单/双大括号不兼容 → AI 评审「盲评」

# 2. 缺陷描述

PR #945（feat 029）把 `compose_review_prompt` 的替换键从 `{original_prompt}` 改为 `{{original_prompt}}`（双大括号），`DEFAULT_REVIEWER_PROMPT` 常量同步改了，但**没有迁移 review_templates 表存量行**。`ensure_default_review_template` 只在缺失时插入、从不更新旧行。旧行（单大括号）与新替换键（双大括号）永久错位。

**影响**：AI 评审实例实际收到的 prompt 中 `{original_prompt}`、`{original_output}`、`{max_output_chars}`、`{acceptance_criteria}` 全部以字面量残留——评审在**看不到原始任务、执行输出、验收标准**的情况下打分，评分仍被解析并驱动门禁，等于门禁形同虚设。

# 3. 影响范围

- **存量实例必现**：任何从旧版升级、且 `review_templates` 表已有默认模板行的实例
- **新装实例不受影响**：直接种入双大括号模板
- **严重度**：P1（功能正确性，线上必现，门禁失效）

# 4. 复现方式

1. 确保 DB 中存在单大括号模板：`sqlite3 ~/.ntd/data.dev.db "SELECT substr(prompt, instr(prompt,'# 原始任务'), 60) FROM review_templates;"`
2. 触发含 ai_criteria_review 门禁的执行
3. 检查评审实例 prompt（证据：`test-evidence/2026-08-01-pr963-967/prompts/prompt-20260801-122117-25077.txt`）

# 5. 修复方案

代码侧同时兼容单大括号和双大括号两种占位符（先双后单），确保存量 DB 行和新模板都能正确替换。不破坏用户自定义模板。

# 6. 关联需求/PR

- PR #945（引入双大括号）、PR #963/#964/#966/#967（验证报告发现）
