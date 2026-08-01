# 0. 文件修改记录表

| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| AI | 2026-08-01 | 初始版本：根因分析 |

# 1. 根因分析

`SelectedSkillTags` 直接 map selected 数组渲染 Tag，未过滤纯空白项。后端 `inject_todo_skills` 已过滤空白，前端展示层未同步。

# 2. 修复策略

`SelectedSkillTags` 渲染前 `filter((n) => n.trim().length > 0)`。
