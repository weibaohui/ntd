# 037-环节面板优化与goto前缀清除-设计

| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| Claude | 2026-07-29 | 初始版本 — 环节属性面板 4 项改动 |

> 分支：`feat/recipe-editor`
> 决策来源：用户交互确认（skills 复用卡片、goto 彻底清除不兼容）

---

## 1. 背景

用户反馈环节属性面板（`LinkPropertyForm`）4 个问题：

1. **位置**：验收标准应放在「提示词」后；执行器应移到面板最后。
2. **缺专家/skills**：后端 `LinkDefinition` 已有 `expert`(mod.rs:127) 和 `skills`(mod.rs:129) 字段且 installer 已使用，但前端环节面板未暴露编辑入口。
3. **goto 前缀冗余**：`on_success`/`on_gate_fail` 的跳转值用 `goto:<id>` 前缀（如 `on_success: goto:write-prd`），前缀冗余，应清除为裸 id（`on_success: write-prd`）。
4. **可空字段缺清空**：下拉类可空字段无法清空所选。

---

## 2. 改动1：位置调整（LinkPropertyForm）

新字段顺序：
```
标识 / 名称 / 提示词 / 验收标准 / 专家 / 技能 / 审核类型 / 评审Prompt /
成功后跳转 / 门禁失败后 / 门禁 / 期望产物 / 环节spec模板 / 执行器(最后)
```
- 验收标准从原末尾区移到「提示词」后。
- 执行器从原第 4 位移到面板最后。

---

## 3. 改动2：专家 + 技能

- **专家**：直接复用 `ExpertPicker`（`frontend/src/components/todo-drawer/ExpertPicker.tsx`），受控 `value=link.expert` / `onChange`。组件自带清除按钮。
- **技能**：复用 `SkillSelector` 卡片 UI（`todo-drawer/SkillSelector.tsx`），但语义不同——todo 是点击插入 `/skill` 到 prompt，环节是 toggle 存 `link.skills[]`。
  - **改造 SkillSelector**：新增可选 prop `selectedSkills?: string[]`，卡片按 `selectedSkills.includes(skill.name)` 显示选中态（边框高亮 + check 图标）。todo 不传该 prop（保持原样），向后兼容。
  - **环节集成**：按环节 `executor` 加载技能（复用 `db.getSkillsList` → 过滤 `executor`），`selectedSkills=link.skills`，`onSkillClick=toggle link.skills`。

---

## 4. 改动3：goto 前缀清除（彻底不兼容）

> 用户决策：彻底清除，不兼容旧 `goto:` 前缀。外部 bundled 系统工艺 yaml 源（不在本仓库）由用户同步更新。

### 统一规则
- 保留字：`next` / `end`（on_success）、`break`（on_gate_fail）、`skip` —— 特殊语义。
- **其他非空值 = 跳转目标环节 id（裸）**。

### 后端
| 文件 | 改动 |
|------|------|
| `services/process/installer.rs::resolve_goto` (L374-403) | 保留字 → `Ok(None)`；`_` 分支改为当环节 id 解析：`template_link_to_step.get(policy)`，找不到 → `GotoTargetNotFound`（原 `_` 是 warn+next 容错，改为严格解析） |
| `services/process/transition_resolver.rs::resolve_by_policy` (L50-91) | 合并 `"goto"` / `starts_with("goto:")` / `_` 三分支为：保留字 next/skip/end/break 特殊处理；`_` 用 `success_goto_step_id`/`fail_goto_step_id` 解析（找不到 fallback next） |
| `installer.rs` 测试 fixture (L754/761) | `on_gate_fail: goto:write-prd` → `on_gate_fail: write-prd` |
| `installer.rs` 测试断言 (L916) | `assert_eq!(... on_rating_fail, "goto:write-prd")` → `"write-prd"` |

### 前端
| 文件 | 改动 |
|------|------|
| `processDefinitionUpdater.ts::isGotoTarget` (L43-47) | `value === \`goto:${id}\`` → `value === targetLinkId`（且非空） |
| `processDefinitionUpdater.ts::setLinkGoto` (L249) | `\`goto:${targetLinkId}\`` → `targetLinkId` |
| `processDefinitionUpdater.ts::resetGotoForTargets` (L288-300) | `startsWith('goto:')+slice` → 直接 `targetSet.has(link.on_success)` |
| `processGraphBuilder.ts` (L193-214) | `startsWith('goto:')+slice` → `link.on_success && !RESERVED.has(...)`，target=值本身 |
| `processFlowAdapter.ts::resolveTransitionTarget` (L187-211) | `next`/`end` 保留；`break`/`skip`→null；`_`→`idMap.get(s)`（裸 id） |
| `processFlowAdapter.ts` 边 label (L131/146) | `startsWith('goto:')?slice(5)` → 直接用裸 id 作 label |
| `LinkPropertyForm.tsx::buildGotoOptions` (L569) | `value: \`goto:${link.id}\`` → `value: link.id` |

> 注：`LoopStudioStepsPanel.tsx` L315/341 的 `value="goto"` 是环路运行时步骤面板的「跳转到指定环节」选项标记，与工艺编辑器 yaml 无关，**不动**。

---

## 5. 改动4：allowClear（可空下拉）

`LinkPropertyForm` 的 Select 加 `allowClear`：
- 成功后跳转（on_success）、门禁失败后（on_gate_fail）、审核类型（review_type）、执行器（executor）。
- 清空 → 字段设 undefined，yaml 省略 → 后端 default（on_success=next / on_gate_fail=break）。
- 专家（ExpertPicker）自带清除按钮，不加 allowClear。

---

## 6. 实现顺序

1. 后端 goto（installer + transition_resolver + 测试）
2. 前端 goto（processDefinitionUpdater + processGraphBuilder + processFlowAdapter + LinkPropertyForm.buildGotoOptions）
3. 环节面板重排 + allowClear
4. 专家 + 技能（SkillSelector 改造 + 集成）
5. 验证 + 实现总结

---

## 7. 验证

- **后端**：`cargo clippy --all-targets -- -D warnings` + `cargo test`（installer goto 解析、transition_resolver 流转）
- **前端**：`npx tsc --noEmit` + `npm run build` + `processDefinitionUpdater.test` 等单测
- **UI（agent-browser）**：goto 下拉选中显示环节名（非 goto:xxx）、保存后 yaml 是裸 id、技能 toggle 选中、专家选择、下拉可清空

---

## 8. 约束遵循

- 单函数 ≤50 行：resolve_goto/resolve_by_policy 改后仍简短。
- 零告警：clippy + tsc。
- 注释：goto 解析规则、保留字集合、为何裸 id（清除前缀）逐处说明。
- SkillSelector 改造向后兼容（selectedSkills 可选）。
