# 0. 文件修改记录表

| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| AI | 2026-08-01 | 初始版本：BUG-008 缺陷说明 |

# 1. 缺陷标题

工艺升级级联删除历史 loop_phase_executions，审计链断裂

# 2. 缺陷描述

`process upgrade` 删除旧 `loop_phases` 重建（id 变化），`loop_phase_executions.phase_id` 外键为 `ON DELETE CASCADE`，导致历史 phase 执行记录被级联删除。audit 对旧执行回显 phase `pending/started_at=None`，与 `loop_execution.status=success` 矛盾。

# 3. 影响范围

升级过的工艺，所有历史 phase 执行记录丢失。P2（审计数据丢失）。

# 4. 修复方案

把 `phase_id` 外键从 `ON DELETE CASCADE` 改为 `ON DELETE SET NULL`，阶段被删时执行记录保留、phase_id 置 NULL。
