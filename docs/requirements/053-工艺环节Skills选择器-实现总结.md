| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| Claude | 2026-07-31 | 初始版本（需求 053 实现总结） |

# 实现总结：工艺环节 Skills 选择器改造（053）

## 1. 背景

环节 `skills` 后端全链路已通，前端旧 `LinkSkillPicker`（卡片 toggle）有 6 个可用性问题导致用户"看不懂"。
本次重做为「内联 table 多选 + 手填 + 内置执行器筛选 + 来源标注」，并让环节执行器可清空。
实现中按用户追加约束调整：**skill 与执行器解耦**（link.skills 存纯名，执行器仅作筛选条件）。

## 2. 改动清单

| 文件 | 改动 |
|------|------|
| `frontend/src/components/process/propertyForms/skillSelectionUtils.ts` | **新建**：7 个纯函数——splitSelected / syncFromTable / canAddCustom / addCustom / removeSkill / filterSkills / skillTagMeta；syncFromTable 内置去重；skillTagMeta 统一标注（去来源，仅区分自定义） |
| `frontend/src/components/process/propertyForms/skillSelectionUtils.test.ts` | **新建**：32 用例（含 skillTagMeta 4 场景 + syncFromTable 去重 2 场景） |
| `frontend/src/components/process/propertyForms/LinkSkillPicker.tsx` | **重写**：props `{selectedSkills, onChange}`（**不读 executor**）；内置 `filterExecutor` 筛选（ExecutorPickerPopover）；全量加载按筛选过滤 Table；**currentSkills 按 name 去重**（后端可能返回重复名）；已选 Tag 用 skillTagMeta 标注来源；跨执行器保留 |
| `frontend/src/components/process/propertyForms/LinkPropertyForm.tsx` | 执行器字段用 ExecutorPickerPopover（可清空，空态显"未选择执行器"）；技能用新 LinkSkillPicker；删 `handleToggleSkill` 与 CloseOutlined；**字段按「身份→执行→产物→评审门禁→流转→模板」语义分组重排** |
| `frontend/src/components/common/ExecutorPickerPopover.tsx` | 扩展：value 空时显 placeholder（不再默认 claudecode）；可选 `onClear` 清空按钮（stopPropagation 不弹下拉）；既有调用方行为不变 |
| `docs/design/053` / `docs/requirements/053` ×2 | 三件套文档 |

**未动**：后端、`getSkillsList()`、todo 侧 `SkillSelector`。

## 3. 验证证据

### 3.1 类型与构建
```
cd frontend && npx tsc --noEmit   → TSC_OK（零错误）
cd frontend && npm run build      → ✓ built（chunk 警告为 monaco/antd 既有）
```

### 3.2 单元测试
```
cd frontend && npx vitest run     → 24 文件 / 199 用例全过（含 skillSelectionUtils 32 用例）
```

### 3.3 端到端（agent-browser，worktree vite @ 5175 → proxy /api 到 18088）

在「标准需求交付工艺」编辑器选中「生成 PRD」环节：

| 验收点 | 结果 |
|--------|------|
| 执行器选/清空 | ✅ 空态显示「未选择执行器」→ 选 Pi → 点清空 → 回到「未选择执行器」 |
| 内置执行器筛选 | ✅ 技能区有独立执行器筛选器；Claude(44) ↔ Pi(4) 切换 Table 正确 |
| 跨执行器保留 | ✅ pi 勾 code-refactoring，切 Claude 后已选 Tag 仍保留 |
| 同名同步 | ✅ pi 勾 browser-use，切 Claude 后 claudecode 的 browser-use 行 checked=true |
| 点选/取消/再勾 | ✅ checkbox 同步已选 Tag、取消再勾正常 |
| 手填/删除 | ✅ 手填回车加「·自定义」Tag、Tag × 删除正常 |
| **skill 重复修复** | ✅ claudecode 表格 code-refactoring 由 2 行去重为 1 行；勾选后已选区仅 1 个 tag；切 kilo 表格显示 Empty「该执行器暂无 Skills」 |
| **标注统一** | ✅ 勾选 browser-use 后，claudecode 与 kilo 筛选下已选 Tag 均无来源标注（统一）；手填自定义稳定显示「·自定义」 |
| **面板排版重排** | ✅ 字段顺序=标识→名称→提示词→执行器→技能→专家→验收标准→期望产物→评审Prompt→门禁→成功后跳转→门禁失败后→最大返工→spec模板 |

> 说明：antd Dropdown 菜单项在 agent-browser 下点击偶有 ref 时效导致切执行器失败（非应用 bug）；
> 来源标注（skillTagMeta）与去重（syncFromTable）逻辑由单测覆盖。

## 4. 已知缺口

- **后端数据重复**：`claudecode` 的 skills 返回 2 个同名 `code-refactoring`（一个 file_count=84，一个 file_count=1，疑似嵌套目录误扫）。前端已按 name 去重兜底；建议后端扫描层同样去重（另立需求）。
- `Database::create_loop_step`（复制 loop 路径）不接收 `skill_names`，复制 loop 时环节 skills 丢失——既有 bug。
- 已选 skill 不在当前筛选执行器时表格不显示（设计如此）；切换执行器筛选即可在表格中管理。
- 暂未提交代码与创建 PR（按约定由用户确认后执行）。
