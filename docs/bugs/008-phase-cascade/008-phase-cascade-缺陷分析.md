# 0. 文件修改记录表

| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| AI | 2026-08-01 | 初始版本：根因分析 |

# 1. 根因分析

- `loop_phase_executions.phase_id` 外键 `ON DELETE CASCADE`
- upgrade 删除旧 phases 时级联删除历史 phase executions
- `loop_step_executions` 无 step_id 外键（仅 loop_execution_id CASCADE + execution_record_id SET NULL），环节历史得以保留

两表外键设计不一致，phase 侧误把「模板行」当「可级联父行」。

# 2. 修复策略

SQLite 不支持 ALTER FOREIGN KEY，重建表改为 `ON DELETE SET NULL`。
