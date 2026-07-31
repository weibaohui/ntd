| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| Claude | 2026-07-31 | 初始版本 |

# 设计：工艺环节 Skills 选择器改造（053）

## 1. 背景

环节 `skills` 字段后端全链路已通，但前端旧 `LinkSkillPicker`（卡片 toggle）有 6 个叠加的可用性问题：
未填执行器就消失、默认折叠、选中态弱、无已选汇总、字段顺序错位（执行器在末尾）、不能手填。
用户反馈"完全看不懂"。本次重做为「内联 table 多选 + 手填」。

**设计过程中用户追加了两个关键约束**：
1. **环节「执行器」可清空，非必选**——不能手填但必须能清空，空态要明确显示。
2. **skill 不绑定执行器**——`link.skills` 存纯技能名（skill 可装到任意执行器），执行器只是
   skills 选择器**内部**的筛选条件；环节属性上的「执行器」（运行用）与它相互独立、并行存在。

## 2. 复用既有资产

| 资产 | 位置 | 用途 |
|------|------|------|
| skills 接口 | `getSkillsList()`（`utils/database/skills.ts:4`） | 全量加载，按筛选执行器过滤 |
| 执行器选择器 | `ExecutorPickerPopover`（`components/common/`，闪念同款） | 环节执行器 + 技能筛选器 |
| 多选表格模式 | 项目 5 处 `Table + rowSelection` | 照此模式勾选 |
| 类型 | `SkillMeta`、`ExecutorSkills`、`LinkDefinition.skills` | 直接使用 |

antd **v6**。**零新接口、零后端改动**。

## 3. 设计决策

### 3.1 环节「执行器」字段：可清空的选择器
- 复用闪念同款 `ExecutorPickerPopover`（只能选不能手填）。
- 给该通用组件扩展：`value` 为空时显示「未选择执行器」占位（不再默认回退 claudecode，避免"看似已选实则为空"误导）+ 可选 `onClear` 显示清空按钮。
- 既有调用方（闪念/动作/聊天）传值恒非空、不传 onClear，**行为不变**（向后兼容）。
- 环节场景传 `onClear={() => handleFieldChange('executor', undefined)}`，清空=空值（运行时用系统默认）。

### 3.2 skills 与执行器解耦 + 内置执行器筛选
- `LinkSkillPicker` **不再接收/读取 `link.executor`**。
- 组件内部 `filterExecutor` state（默认 `DEFAULT_EXECUTOR`），由内置 `ExecutorPickerPopover` 切换；
  **仅决定 Table 展示哪个执行器的可选 skills，不影响已选**。
- 全量加载 `getSkillsList()`；`currentSkills = list.find(executor === filterExecutor)`。
- 已选 `link.skills` 是纯名字数组，**跨执行器保留**（切换筛选不丢）。

### 3.3 已选 ↔ 表格选中桥接（核心难点）
`link.skills` 来源两类：当前筛选执行器勾选（精确等于列表名）+ 手填/其他执行器的已选。
多选 Table 只承载前者（`rowKey=name`），其余活在已选 Tag 区。桥接抽成纯函数（`skillSelectionUtils.ts`）：

- `splitSelected` / `syncFromTable`：按**当前筛选执行器名集合**拆分与合并，保证切换筛选不误删已选。
- `canAddCustom` / `addCustom` / `removeSkill`：手填去重、删除。
- `filterSkills`：name/description/keywords 过滤。
- `skillTagMeta`：已选 Tag **统一标注**——全量执行器都没有=手填自定义（橙「·自定义」）；已知 skill=蓝无标注。
  标注稳定、与筛选执行器无关（skill 不绑定执行器，来源标注无意义且随切换时有时无，故统一为不显示来源）。

### 3.4 内联展开布局

```
Form.Item「执行器」：ExecutorPickerPopover（可清空，空态显"未选择执行器"）
Form.Item「技能」：
 ├ 已选 Tag 区（来源标注）
 ├ [执行器筛选 ExecutorPickerPopover] [搜索框(回车手填)]
 └ Table：技能(name+描述) | 版本     ← 复选框多选，pageSize 8
```

### 3.5 函数长度控制
拆分：`useAllSkills` hook、`SelectedSkillTags` 子组件、`SKILL_COLUMNS` 常量、`skillSelectionUtils` 纯函数。

## 4. 范围边界

不改后端 / 不新接口 / 不动 todo 侧 `SkillSelector` / 不引入 Transfer 等 / 不处理复制 loop 丢 skill_names 的既有缺口。

## 5. 改动清单

| 文件 | 改动 |
|------|------|
| `frontend/src/components/process/propertyForms/skillSelectionUtils.ts` | **新建**：7 个纯函数（含 skillTagMeta） |
| `frontend/src/components/process/propertyForms/skillSelectionUtils.test.ts` | **新建**：30 用例 |
| `frontend/src/components/process/propertyForms/LinkSkillPicker.tsx` | **重写**：解耦 executor + 内置执行器筛选 + 来源标注 + Table 多选 + 手填 |
| `frontend/src/components/process/propertyForms/LinkPropertyForm.tsx` | 执行器字段用 ExecutorPickerPopover（可清空）；技能用新 LinkSkillPicker；删除旧 handler |
| `frontend/src/components/common/ExecutorPickerPopover.tsx` | 扩展：空值占位 + 可选 onClear（向后兼容） |
| `docs/design/053` / `docs/requirements/053` ×2 | 三件套文档 |

## 6. 验证

1. `npx tsc --noEmit` 零错误；`npm run build` 无新告警。
2. `npx vitest run` 纯函数 30 用例 + 全量回归通过。
3. agent-browser 端到端：执行器选/清空、筛选器切换 Table、跨执行器保留、同名同步、手填、删除（详见实现总结 §3）。
