# 037-环节面板优化与goto前缀清除-实现总结

| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| Claude | 2026-07-29 | 初始版本 — 环节面板 4 项改动 + goto 前缀清除 |

> 设计：`docs/design/037-环节面板优化与goto前缀清除-设计.md`
> 分支：`feat/recipe-editor`

---

## 1. 概述

按用户反馈对环节属性面板（`LinkPropertyForm`）做 4 项改动：
1. 验收标准移到「提示词」后；执行器移到面板最后。
2. 新增专家（复用 `ExpertPicker`）+ 技能（复用 `SkillSelector` 卡片，toggle 存 `link.skills[]`）。
3. 清除 `on_success`/`on_gate_fail` 的 `goto:` 前缀，跳转值用裸环节 id（彻底不兼容旧前缀）。
4. 可空下拉加 `allowClear` 可清空。

---

## 2. 改动清单

### goto 前缀清除（前后端语义）

| 文件 | 改动 |
|------|------|
| `backend/services/process/installer.rs::resolve_goto` | 保留字 next/end/break/skip → None；其他当裸环节 id 解析（找不到报 GotoTargetNotFound，不再容错 next） |
| `backend/services/process/transition_resolver.rs::resolve_by_policy` | 合并 goto/goto:/_ 三分支为：保留字特殊 + 默认用 success_goto_step_id/fail_goto_step_id |
| `backend/services/process/installer.rs` 测试 | fixture/断言 `goto:write-prd` → `write-prd` |
| `backend/services/process/transition_resolver.rs` 测试 | policy 字面量 `"goto"` → 裸 id `"step-3"` |
| `frontend/.../processDefinitionUpdater.ts` | `isGotoTarget`/`setLinkGoto`/`resetGotoForTargets` 去 `goto:` 前缀，直接比较裸 id |
| `frontend/.../processGraphBuilder.ts` | 加 `TRANSITION_RESERVED` 常量；跳转判断改为"非保留字即裸 id" |
| `frontend/.../processFlowAdapter.ts` | `resolveTransitionTarget` + 边 label 去 `goto:` 前缀 |
| `frontend/.../LinkPropertyForm.tsx::buildGotoOptions` | 选项 value 从 `goto:${id}` → 裸 `id` |
| 前端单测 | `processDefinitionUpdater.test`/`processGraphBuilder.test` 的 `goto:link2`/`goto:nonexistent` → 裸 id |

### 环节面板重排 + allowClear + 专家技能

| 文件 | 改动 |
|------|------|
| `frontend/.../propertyForms/LinkPropertyForm.tsx` | return 重排：标识/名称/提示词/**验收标准**/**专家**/**技能**/审核类型/评审Prompt/成功后跳转/门禁失败后/门禁/产物/spec模板/**执行器**(最后)；Select 加 `allowClear` + placeholder；执行器 Input 加 suffix 清除按钮；加 `handleToggleSkill` |
| `frontend/.../todo-drawer/SkillSelector.tsx` | 新增可选 prop `selectedSkills?: string[]`，卡片按选中态高亮（边框/背景/check 图标），向后兼容 todo 用法 |
| `frontend/.../propertyForms/LinkSkillPicker.tsx`（新增） | 封装：按环节 executor 加载技能（`getSkillsList`），复用 SkillSelector 卡片，点击 toggle 存 `link.skills[]` |

---

## 3. 验证结果（本次实测）

| 验证项 | 命令 | 结果 |
|--------|------|------|
| 后端 lint | `cargo clippy --all-targets -- -D warnings` | ✅ 零告警 |
| 后端单测 | `cargo test --lib installer` / `transition_resolver` | ✅ 8 passed / 7 passed |
| 前端类型 | `npx tsc --noEmit` | ✅ exit 0 |
| 前端单测 | `npx vitest run src/components/process` | ✅ 63 passed |
| 前端构建 | `npm run build` | ✅ exit 0 |
| UI（agent-browser） | 见下 | ✅ |

### UI 验证（dev :18088）

- 环节属性面板新布局：标识→名称→提示词→**验收标准**→**专家(选择专家/团队)**→技能→审核类型→评审Prompt→跳转→门禁失败→表格→**执行器(最后)** ✅
- 专家：`ExpertPicker` 渲染，可点开选择 ✅
- 技能：环节填 executor 后出现「Skills N 个可用」卡片（`LinkSkillPicker` 按 executor 加载）✅
- goto 裸 id：选「成功后跳转=拆解用户故事」后，YAML 写入 `on_success: req-01b`（裸 id，无 `goto:` 前缀）✅

---

## 4. 待用户处理：bundled 系统工艺 yaml 同步

用户选定「彻底清除不兼容」。本仓库代码已不识别 `goto:` 前缀，但 **bundled 系统工艺的 yaml 源不在本仓库**（外部资源同步进 `~/.ntd/bundled`），其 yaml 仍用旧 `goto:` 格式（如 `on_gate_fail: goto:write-prd`）。

- **影响**：未同步前，重新安装这些系统工艺会在 `resolve_goto` 报 `GotoTargetNotFound`（旧 `goto:write-prd` 被当作不存在的环节 id）。
- **处理**：需同步更新外部 bundled 工艺 yaml 源，把 `goto:<id>` 改为裸 `<id>`。UI 层旧 `goto:req-01` 当前显示为原始文本（Select 找不到匹配选项），同步后即正常。

---

## 5. 约束遵循

- SkillSelector 改造向后兼容（`selectedSkills` 可选，todo 不传保持原"点击插入 prompt"行为）。
- 单函数 ≤50 行：`resolve_goto`/`resolve_by_policy` 改后仍简短；技能加载逻辑抽到 `LinkSkillPicker`，避免 `LinkPropertyForm` 进一步膨胀。
- 零告警：clippy + tsc + build。
- 注释：goto 解析规则（保留字集合、裸 id 语义）逐处说明。
