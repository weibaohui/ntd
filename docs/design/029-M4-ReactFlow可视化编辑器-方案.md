# 029-M4 React Flow 可视化编辑器 — 方案文档

| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| AtomCode (GLM-5.2) | 2026-07-27 | 初始版本，M4 里程碑实施方案 |

---

## 1. 背景与定位

本方案是 029 工艺编辑器的第 4 个里程碑（M4），对应：
- 需求文档：`docs/requirements/029-工艺模板编辑与可视化创建-需求.md` §3.3（可视化泳道编辑器）
- 设计文档：`docs/design/029-工艺模板编辑与可视化创建-设计.md` §5（可视化编辑器）+ §7（属性面板）

### 1.1 里程碑进度

| 里程碑 | 状态 | Commit | 说明 |
|--------|------|--------|------|
| M1 后端 API | ✅ | a14da299 | PUT/POST/DELETE + 13 单元测试 |
| M2 前端路由与入口 | ✅ | d44b6a30 | `useViewState` 支持 `processEditor`，`ProcessPage` 占位 |
| M3 Monaco YAML 编辑器 | ✅ | e931ec8f | Monaco 集成 + js-yaml 实时校验 + 错误标记 |
| **M4 React Flow 可视化编辑器** | **⏳ 本方案** | — | 泳道编辑器 + 属性面板 |
| M5 双向联动与保存 | 待做 | — | sync flag + 保存/删除按钮 |
| M6 新建工艺流程 | 待做 | — | 元信息 Modal + 空工艺渲染 |
| M7 编译告警清理与测试 | 待做 | — | 全量验证 |

### 1.2 M4 的边界

**做**（对应设计 §5 + §7）：

1. **安装 React Flow 依赖**（`@xyflow/react`；`dagre`/`@types/dagre` 已在 `package.json`）
2. **自定义节点**：`PhaseNode`（泳道容器）+ `LinkNode`（环节卡片）
3. **连线模型**：双 source handle（绿=`on_success`，橙=`on_gate_fail`）+ `onConnect` 更新 `ProcessDefinition` + 自定义 edge 带 hover 删除按钮
4. **属性面板**：`LinkPropertyForm`（8 字段 + gates/expected_artifacts 嵌套表格）+ `PhasePropertyForm`（6 字段）+ `GlobalPropertyForm`（limits/abnormal_handler/step_templates）
5. **删除节点级联处理**：拦截 `onNodesDelete`，弹 `Modal.confirm`，重置悬空 goto 引用
6. **布局算法**：dagre 横向布局 phase，phase 内部 flex 纵向排列 link
7. **画布导航**：`fitView` + `MiniMap` + `Controls` + 新增 phase 自动 `fitView({ nodes: [newPhaseId] })`

**不做**（留给后续里程碑）：

- **YAML ↔ 可视化双向联动**（M5）— M4 的 React Flow 只读渲染当前 `ProcessDefinition`，可视化操作（增删节点、改属性、拖连线）通过回调更新父组件的 `ProcessDefinition` 对象，但**不回写 Monaco**（M5 用 sync flag 实现）
- **保存/删除按钮**（M5）— M4 只做可视化编辑，不调用后端 API
- **新建工艺元信息 Modal**（M6）— M4 处理已有工艺的可视化
- **空工艺渲染**（M6）— M4 假设 `definition.phases` 已有数据或渲染空画布
- **离开拦截**（M5）— `useBlocker` + `beforeunload`

### 1.3 关键现状核对

| 项 | 现状 | M4 处理 |
|----|------|---------|
| `@xyflow/react` | ❌ 未安装 | M4 新增 |
| `dagre` / `@types/dagre` | ✅ 已在 `package.json`（`^0.8.5` / `^0.7.54`） | 直接复用 |
| `types/process.ts` | ✅ 已定义 `ProcessDefinition` / `PhaseDefinition` / `LinkDefinition` / `GateDefinition` / `ExpectedArtifact` / `ProcessMeta` / `ProcessLimits` | M4 直接消费 |
| M3 `ProcessEditor.tsx` | ✅ M3 骨架（加载工艺 → Alert → Monaco） | M4 扩展：加可视化区 + 属性面板 |
| M3 `ProcessYamlEditor.tsx` | ✅ Monaco 封装 | M4 不动 |
| `ProcessFlowGraph` | ✅ 既有只读流程图（dagre + 手写 SVG） | M4 不动，M4 是新的可编辑 React Flow |

---

## 2. 技术决策

### 2.1 React Flow v12 vs v11

**决策**：用 `@xyflow/react` ^12.3.0（React Flow v12 的新包名，v11 叫 `reactflow`）。

**理由**（需求 §3.3.1）：
- v12 是当前稳定版，官方推荐新项目用 `@xyflow/react`
- v12 支持子节点（`parentNode`）原生拖拽，无需额外配置
- 团队已有 dagre 经验（`useFlowLayout`），React Flow 文档完善

**风险**：v12 的 `Handle` / `Edge` API 与 v11 略有差异，需查官方文档。

### 2.2 节点模型：group 节点 + parentNode

**决策**（设计 §5.2.1）：
- `PhaseNode` 是 group 容器，`position` + `width` + `height` 定义边界
- `LinkNode` 通过 `parentNode: phaseId` 挂到 PhaseNode 下
- React Flow 自动处理"拖动父节点移动子节点"

**节点 ID 约定**：
- phase 节点 id：`phase-${phaseIndex}`（如 `phase-0`、`phase-1`）
- link 节点 id：`link-${phaseIndex}-${linkIndex}`（如 `link-0-0`、`link-1-2`）
- 边 id：`edge-${sourceNodeId}-${sourceHandle}-${targetNodeId}`（如 `edge-link-0-0-on_success-link-1-0`）

**ID 稳定性**：M4 用 phase/link 数组索引生成节点 id，索引在增删时会变。M4 接受这一限制（YAGNI），用户增删节点后 React Flow 会重新渲染。M5 可考虑用 `phase.id` / `link.id` 作为节点 id（更稳定）。

### 2.3 连线语义：双 source handle 颜色编码

**决策**（设计 §5.4 + 需求 §3.3.3）：
- LinkNode 右侧两个 source handle：
  - 上方绿色 handle，id=`on_success`，拖出线更新 `on_success: goto:<target_id>`
  - 下方橙色 handle，id=`on_gate_fail`，拖出线更新 `on_gate_fail: goto:<target_id>`
- LinkNode 左侧一个 target handle，id=`target`，连线终点

**边视觉**（设计 §5.4.1）：

| 边类型 | 触发条件 | 视觉 |
|--------|---------|------|
| 顺向 success 边 | `on_success: next` 或 `on_success: end` | 灰色 `#94a3b8` smoothstep |
| goto 成功边 | `on_success: goto:xxx` | 绿色 `#10b981` smoothstep |
| goto 门禁失败边 | `on_gate_fail: goto:xxx` | 橙色虚线 `#d97706` smoothstep |

**onConnect 处理**（设计 §5.4.2）：
```typescript
function handleConnect(connection: Connection) {
  // connection.sourceHandle 区分 on_success / on_gate_fail
  const handleType = connection.sourceHandle;  // 'on_success' | 'on_gate_fail'
  const targetNodeId = connection.target;       // 'link-${phaseIndex}-${linkIndex}'
  const targetLink = findLinkByNodeId(targetNodeId);
  const targetId = targetLink?.id;              // link.id（YAML 里的环节 id）

  if (!targetId) return;

  // 更新 ProcessDefinition 对应 link 的 on_success / on_gate_fail
  const newDefinition = updateLinkField(
    definition,
    sourceLinkPhaseIndex,
    sourceLinkIndex,
    handleType,           // 'on_success' | 'on_gate_fail'
    `goto:${targetId}`
  );
  onDefinitionChange(newDefinition);
}
```

### 2.4 删除连线：hover 浮现叉号

**决策**（设计 §5.4.3）：自定义 edge 组件，hover 时在中点浮现小叉号按钮，点击删除。

**删除语义**：
- 删 `on_success: goto:xxx` → `on_success` 改回 `next`
- 删 `on_gate_fail: goto:xxx` → `on_gate_fail` 改回 `break`

### 2.5 删除节点级联处理

**决策**（设计 §5.5 + 需求 §3.3.6）：

#### 2.5.1 删除 link 节点

拦截 React Flow `onNodesDelete`，弹 `Modal.confirm`：
```
删除环节「<link.name>」？
有 N 处其他环节的跳转引用了此环节，将被重置为 next/break。
```
确认后：
1. 从 `phases[].links[]` 移除该 link
2. 遍历所有 link，把 `on_success: goto:<被删link.id>` 重置为 `next`
3. 把 `on_gate_fail: goto:<被删link.id>` 重置为 `break`

#### 2.5.2 删除 phase 容器

泳道头部删除按钮点击，弹 `Modal.confirm`：
```
删除阶段「<phase.name>」？
将删除本阶段及其下 N 个环节。有 M 处其他环节的跳转引用了本阶段的环节，将被重置为 next/break。
```
确认后：遍历 phase 下所有 link，对每一个执行上述"删除 link 时重置悬空 goto"的级联重置。

### 2.6 布局算法

**决策**（设计 §5.3）：两层布局。

1. **横向布局（dagre）**：按 `phases` 数组顺序从左到右排列 PhaseNode，`rankdir: 'LR'`，`ranksep: 80`
2. **纵向布局（phase 内部 flex）**：每个 PhaseNode 内部用 CSS flex 纵向排列 LinkNode，间距固定 16px，头部 60px

**节点尺寸常量**：
- `NODE_WIDTH = 240`（LinkNode 卡片宽度）
- `NODE_HEIGHT = 80`（LinkNode 卡片高度）
- `PHASE_PADDING = 40`（PhaseNode 内边距）
- `PHASE_HEADER = 60`（PhaseNode 头部高度）
- `LINK_GAP = 16`（LinkNode 之间间距）

### 2.7 属性面板

**决策**（设计 §7）：右侧属性面板，根据选中节点切换。

#### 2.7.1 LinkPropertyForm（环节属性面板，8 字段 + 嵌套表格）

| 字段 | 控件 | 校验 |
|------|------|------|
| `id` | Input | onChange 实时校验：合法字符 `^[a-zA-Z0-9_-]+$` + 全局唯一 |
| `name` | Input | 必填 |
| `step_template` | Select | 选项来自 `GET /processes/step-templates`（M4 先用空选项占位，M5 接真实接口） |
| `prompt` | Input.TextArea | 可空 |
| `executor` | Input | 可空 |
| `review_type` | Select: `ai` / `human` | 默认 `ai` |
| `on_success` | 分组 Select（OptGroup by phase） | 选项：`next` / `end` / `goto:<link_id>` |
| `on_gate_fail` | 分组 Select（OptGroup by phase） | 选项：`break` / `goto:<link_id>` |

**嵌套字段**（设计 §7.1）：

| 字段 | UI |
|------|-----|
| `gates` | Ant Design Table，可增删行，每行内联 Input/Select 编辑 `name, type, artifact, criteria_ref, min_score, script` |
| `expected_artifacts` | Ant Design Table，可增删行，每行内联 Input 编辑 `name, type, path, locator` |

#### 2.7.2 PhasePropertyForm（阶段属性面板，6 字段）

| 字段 | 控件 |
|------|------|
| `id` | Input + 实时校验 |
| `name` | Input |
| `spec` | Input.TextArea（阶段规范） |
| `acceptance_criteria` | Input.TextArea（验收标准） |
| `acceptance_criteria_ref` | Input（外部验收标准文件引用，可空） |

#### 2.7.3 GlobalPropertyForm（全局面板）

**顶部折叠面板**（设计 §7.3）— 工艺元信息：

| 字段 | 控件 |
|------|------|
| `name` | 只读（创建后不可改） |
| `display_name` | Input |
| `category` | Input 或 Select |
| `complexity` | Select: `light` / `standard` / `complex` |
| `version` | Input，默认 `1.0.0` |
| `description` | Input.TextArea |

**全局面板**：

| 字段 | UI 形态 |
|------|---------|
| `limits` | 小表单：`max_step_executions` (InputNumber) + `max_total_tokens` (InputNumber) |
| `abnormal_handler` | 静态表单：`trigger_on` (多选 Checkbox Group: `capped_step` / `capped_token` / `failed`) |
| `step_templates` | Collapse 折叠面板：每个原型是一个折叠项，展开编辑 `name, prompt, skills, model, executor` |

### 2.8 M4 的 `ProcessEditor` 扩展结构

```
ProcessEditor (M3 骨架，M4 扩展)
├── 顶部 Alert（M3 已实现）
├── ProcessVisualEditor (M4 新增，React Flow 可视化区)
│   ├── ReactFlow (主画布)
│   │   ├── PhaseNode (自定义节点，横向泳道容器)
│   │   ├── LinkNode (自定义节点，环节卡片)
│   │   └── ProcessEdge (自定义边，hover 删除)
│   ├── MiniMap
│   └── Controls
├── ProcessPropertyPanel (M4 新增，右侧属性面板)
│   ├── LinkPropertyForm (8 字段 + gates/expected_artifacts 嵌套表格)
│   ├── PhasePropertyForm (6 字段)
│   └── GlobalPropertyForm (limits/abnormal_handler/step_templates)
└── ProcessYamlEditor (M3 已实现，Monaco YAML 编辑器)
```

**M4 的 `ProcessEditor` 状态扩展**：

```typescript
// M3 已有
const [detail, setDetail] = useState<ProcessTemplateDetail | null>(null);
const [loading, setLoading] = useState(true);
const [yamlText, setYamlText] = useState('');
const [isSystem, setIsSystem] = useState(false);

// M4 新增
// 工艺定义对象（source of truth，YAML ↔ 可视化双向联动的共享对象）
const [definition, setDefinition] = useState<ProcessDefinition | null>(null);
// 当前选中的节点 id（用于属性面板切换）
const [selectedNodeId, setSelectedNodeId] = useState<string | null>(null);
```

**M4 不做的状态**（留 M5）：
- `isSyncing` flag（双向联动防循环）
- `isDirty` 标记（离开拦截）
- `isSaving` 保存状态

---

## 3. 文件改动清单

### 3.1 新增文件

#### 3.1.1 `frontend/src/components/process/nodes/PhaseNode.tsx`

**职责**：渲染横向泳道容器，头部显示 `phase.name` + 删除按钮，内部纵向排列 LinkNode 子节点。

**接口**：
```typescript
interface PhaseNodeData {
  phase: PhaseDefinition;
  phaseIndex: number;
  onDeletePhase: (phaseId: string) => void;
  onSelectPhase: (phaseId: string) => void;
}
```

**实现要点**：
- React Flow group 节点语义
- 容器背景：`rgba(phaseColor, 0.05)`，边框：`1px dashed phaseColor`
- 头部：`▸ {phase.name}` + 右上角小叉号删除按钮
- 内部：纵向 flex 布局（React Flow 会自动定位子节点）

#### 3.1.2 `frontend/src/components/process/nodes/LinkNode.tsx`

**职责**：渲染环节卡片，显示 `link.name` + `step_template`，右侧两个 handle 供连线。

**接口**：
```typescript
interface LinkNodeData {
  link: LinkDefinition;
  phaseId: string;
  phaseIndex: number;
  linkIndex: number;
  onSelectLink: (linkId: string) => void;
}
```

**实现要点**：
- 卡片宽度 240px，高度自适应（~80px）
- 右侧两个 source handle：
  - 上方绿色 handle，id=`on_success`
  - 下方橙色 handle，id=`on_gate_fail`
- 左侧一个 target handle，id=`target`
- 点击卡片 → 选中 link → 右侧属性面板切换

#### 3.1.3 `frontend/src/components/process/nodes/ProcessEdge.tsx`

**职责**：自定义边组件，hover 时在中点浮现小叉号按钮，点击删除。

**接口**：
```typescript
interface ProcessEdgeData {
  color: string;
  dashed: boolean;
  onDelete: (edgeId: string) => void;
}
```

**实现要点**：
- 用 `getSmoothStepPath` 生成路径
- hover 时在中点显示 `<circle>` + `<text>×</text>`
- 点击叉号 → 调用 `onDelete(edgeId)`

#### 3.1.4 `frontend/src/components/process/processLayout.ts`

**职责**：布局算法纯函数模块。

**接口**：
```typescript
// 横向布局 phase（dagre LR）
export function layoutPhases(
  phases: PhaseDefinition[],
  options: { nodeWidth: number; nodeHeight: number; ranksep: number }
): Map<string, { x: number; y: number }>;

// 纵向布局 phase 内部 link（flex）
export function layoutLinksInPhase(
  phase: PhaseDefinition,
  phaseIndex: number
): Array<{ id: string; position: { x: number; y: number } }>;
```

**单元测试**（vitest）：
- `layoutPhases_emptyPhases_returnsEmptyMap`
- `layoutPhases_singlePhase_returnsZeroPosition`
- `layoutPhases_multiplePhases_returnsLeftToRightPositions`
- `layoutLinksInPhase_emptyLinks_returnsEmptyArray`
- `layoutLinksInPhase_multipleLinks_returnsTopToBottomPositions`

#### 3.1.5 `frontend/src/components/process/processGraphBuilder.ts`

**职责**：从 `ProcessDefinition` 构建 React Flow nodes + edges 的纯函数模块。

**接口**：
```typescript
export interface ProcessGraph {
  nodes: Node[];
  edges: Edge[];
}

// 从 ProcessDefinition 构建 React Flow 图
export function buildProcessGraph(
  definition: ProcessDefinition,
  callbacks: {
    onDeletePhase: (phaseId: string) => void;
    onSelectPhase: (phaseId: string) => void;
    onSelectLink: (linkId: string) => void;
    onDeleteEdge: (edgeId: string) => void;
  }
): ProcessGraph;
```

**单元测试**（vitest）：
- `buildProcessGraph_emptyDefinition_returnsEmptyGraph`
- `buildProcessGraph_singlePhaseWithLinks_returnsNodesAndEdges`
- `buildProcessGraph_gotoEdge_returnsGreenEdge`
- `buildProcessGraph_gateFailGotoEdge_returnsOrangeDashedEdge`

#### 3.1.6 `frontend/src/components/process/processDefinitionUpdater.ts`

**职责**：`ProcessDefinition` 不可变更新纯函数模块（增删 phase/link、改属性、拖连线、删除节点级联重置）。

**接口**：
```typescript
// 新增 phase
export function addPhase(definition: ProcessDefinition, phase: PhaseDefinition): ProcessDefinition;
// 删除 phase（级联重置悬空 goto）
export function removePhase(definition: ProcessDefinition, phaseId: string): ProcessDefinition;
// 新增 link
export function addLink(definition: ProcessDefinition, phaseId: string, link: LinkDefinition): ProcessDefinition;
// 删除 link（级联重置悬空 goto）
export function removeLink(definition: ProcessDefinition, linkId: string): ProcessDefinition;
// 更新 link 字段
export function updateLinkField(
  definition: ProcessDefinition,
  phaseId: string,
  linkId: string,
  field: keyof LinkDefinition,
  value: unknown
): ProcessDefinition;
// 更新 phase 字段
export function updatePhaseField(
  definition: ProcessDefinition,
  phaseId: string,
  field: keyof PhaseDefinition,
  value: unknown
): ProcessDefinition;
// 拖连线后更新 on_success / on_gate_fail
export function setLinkGoto(
  definition: ProcessDefinition,
  sourcePhaseId: string,
  sourceLinkId: string,
  handleType: 'on_success' | 'on_gate_fail',
  targetLinkId: string
): ProcessDefinition;
// 删除连线后重置 on_success / on_gate_fail
export function resetLinkGoto(
  definition: ProcessDefinition,
  sourcePhaseId: string,
  sourceLinkId: string,
  handleType: 'on_success' | 'on_gate_fail'
): ProcessDefinition;
// 查找所有引用指定 link 的 goto
export function findGotoReferrers(definition: ProcessDefinition, linkId: string): Array<{ phaseId: string; linkId: string; field: 'on_success' | 'on_gate_fail' }>;
// 查找所有引用指定 phase 下 link 的 goto
export function findGotoReferrersForPhase(definition: ProcessDefinition, phaseId: string): Array<{ phaseId: string; linkId: string; field: 'on_success' | 'on_gate_fail' }>;
```

**单元测试**（vitest）：
- `addPhase_emptyPhases_addsPhase`
- `removePhase_withGotoReferrers_resetsGoto`
- `addLink_toPhase_addsLink`
- `removeLink_withGotoReferrers_resetsGoto`
- `updateLinkField_changesFieldValue`
- `setLinkGoto_setsGotoTarget`
- `resetLinkGoto_resetsToDefault`
- `findGotoReferrers_findsAllReferrers`
- `findGotoReferrersForPhase_findsAllReferrersInPhase`

#### 3.1.7 `frontend/src/components/process/ProcessVisualEditor.tsx`

**职责**：React Flow 主画布组件。

**接口**：
```typescript
interface ProcessVisualEditorProps {
  definition: ProcessDefinition;
  onDefinitionChange: (newDefinition: ProcessDefinition) => void;
  selectedNodeId: string | null;
  onSelectNode: (nodeId: string | null) => void;
  theme: 'dark' | 'light';
}
```

**实现要点**：
- 用 `@xyflow/react` 的 `ReactFlow` 组件
- 注册自定义节点类型：`{ phase: PhaseNode, link: LinkNode }`
- 注册自定义边类型：`{ process: ProcessEdge }`
- `useNodesState` / `useEdgesState` 管理节点和边
- `onConnect` → `setLinkGoto` → `onDefinitionChange`
- `onNodesDelete` → `removeLink` → `onDefinitionChange`
- `MiniMap` + `Controls` + `Background`
- `fitView` 初始视口

#### 3.1.8 `frontend/src/components/process/ProcessPropertyPanel.tsx`

**职责**：右侧属性面板，根据选中节点切换。

**接口**：
```typescript
interface ProcessPropertyPanelProps {
  definition: ProcessDefinition;
  selectedNodeId: string | null;
  onDefinitionChange: (newDefinition: ProcessDefinition) => void;
}
```

**实现要点**：
- `selectedNodeId` 解析：`phase-${i}` → phase，`link-${i}-${j}` → link，`null` → global
- phase → `PhasePropertyForm`
- link → `LinkPropertyForm`
- null → `GlobalPropertyForm`

#### 3.1.9 `frontend/src/components/process/propertyForms/LinkPropertyForm.tsx`

**职责**：环节属性面板（8 字段 + gates/expected_artifacts 嵌套表格）。

**接口**：
```typescript
interface LinkPropertyFormProps {
  definition: ProcessDefinition;
  phaseId: string;
  linkId: string;
  onDefinitionChange: (newDefinition: ProcessDefinition) => void;
}
```

**实现要点**：
- 8 个字段表单（id/name/step_template/prompt/executor/review_type/on_success/on_gate_fail）
- `on_success` / `on_gate_fail` 分组下拉（OptGroup by phase）
- `gates` 嵌套表格（Ant Design Table，可增删行）
- `expected_artifacts` 嵌套表格
- 每个字段 onChange → `updateLinkField` → `onDefinitionChange`

#### 3.1.10 `frontend/src/components/process/propertyForms/PhasePropertyForm.tsx`

**职责**：阶段属性面板（6 字段）。

**接口**：
```typescript
interface PhasePropertyFormProps {
  definition: ProcessDefinition;
  phaseId: string;
  onDefinitionChange: (newDefinition: ProcessDefinition) => void;
}
```

**实现要点**：
- 6 个字段表单（id/name/spec/acceptance_criteria/acceptance_criteria_ref）
- 每个字段 onChange → `updatePhaseField` → `onDefinitionChange`

#### 3.1.11 `frontend/src/components/process/propertyForms/GlobalPropertyForm.tsx`

**职责**：全局面板（limits/abnormal_handler/step_templates + 顶部工艺元信息）。

**接口**：
```typescript
interface GlobalPropertyFormProps {
  definition: ProcessDefinition;
  onDefinitionChange: (newDefinition: ProcessDefinition) => void;
}
```

**实现要点**：
- 顶部折叠面板：工艺元信息（name/display_name/category/complexity/version/description）
- `limits` 小表单
- `abnormal_handler` 静态表单
- `step_templates` Collapse 折叠面板

### 3.2 修改文件

#### 3.2.1 `frontend/package.json`

新增依赖：
```json
{
  "@xyflow/react": "^12.3.0"
}
```

**安装方式**：`cd frontend && npm install @xyflow/react@^12.3.0 --registry=https://registry.npmmirror.com`

**注**：`dagre` / `@types/dagre` 已在 `package.json`，无需重复安装。

#### 3.2.2 `frontend/src/components/process/ProcessEditor.tsx`

**改动点**：M3 骨架扩展为 M4 双栏布局（左可视化 + 右属性面板）。

**M3 当前结构**：
```tsx
<div style={editorContainerStyle}>
  {isSystem ? <Alert ... /> : <Alert ... />}
  <div style={monacoWrapperStyle}>
    <ProcessYamlEditor ... />
  </div>
</div>
```

**M4 改动**：
```tsx
<div style={editorContainerStyle}>
  {isSystem ? <Alert ... /> : <Alert ... />}
  <div style={splitViewStyle}>  {/* 左右分栏 */}
    <div style={visualEditorStyle}>  {/* 左：可视化区 */}
      <ProcessVisualEditor
        definition={definition!}
        onDefinitionChange={handleDefinitionChange}
        selectedNodeId={selectedNodeId}
        onSelectNode={setSelectedNodeId}
        theme={themeMode}
      />
    </div>
    <div style={propertyPanelStyle}>  {/* 右：属性面板 */}
      <ProcessPropertyPanel
        definition={definition!}
        selectedNodeId={selectedNodeId}
        onDefinitionChange={handleDefinitionChange}
      />
    </div>
  </div>
  {/* M3 的 ProcessYamlEditor 在 M5 会用 Tabs 切换可视化/YAML */}
</div>
```

**M4 的 `ProcessEditor` 状态扩展**：
```typescript
// M4 新增
const [definition, setDefinition] = useState<ProcessDefinition | null>(null);
const [selectedNodeId, setSelectedNodeId] = useState<string | null>(null);

// M4 新增：可视化操作回调
const handleDefinitionChange = useCallback((newDefinition: ProcessDefinition) => {
  setDefinition(newDefinition);
  // M5 会在这里加 yaml.dump 刷新 Monaco
}, []);
```

**M4 加载工艺后初始化 `definition`**：
```typescript
// 在 loadDetail 成功后
const result = await bundledApi.getProcess(processName);
setDetail(result);
setYamlText(result.definition);
setIsSystem(result.is_system);
// M4 新增：解析 YAML 文本为 ProcessDefinition 对象
const parsed = parseYaml(result.definition);
if (parsed.parsed && typeof parsed.parsed === 'object') {
  setDefinition(parsed.parsed as ProcessDefinition);
}
```

### 3.3 不改动的文件（M4 范围外）

- `backend/` — M1 已完成，M4 不动后端
- `useViewState.ts` — M2 已实现路由解析，M4 不动
- `App.tsx` — M2 已接入 `processEditor` 视图，M4 不动
- `ProcessYamlEditor.tsx` — M3 已实现 Monaco 封装，M4 不动
- `processYamlValidator.ts` — M3 已实现 js-yaml 解析，M4 直接复用

---

## 4. 数据流

### 4.1 M4 数据流（可视化 → ProcessDefinition，单向）

```
ProcessEditor (父，维护 definition + selectedNodeId)
  ↓ definition
ProcessVisualEditor (React Flow)
  ├─ buildProcessGraph(definition, callbacks) → nodes + edges
  ├─ onConnect(connection) → setLinkGoto(definition, ...) → onDefinitionChange(newDefinition)
  ├─ onNodesDelete(deletedNodes) → removeLink(definition, ...) → onDefinitionChange(newDefinition)
  └─ onSelectNode(nodeId) → setSelectedNodeId
  ↓ definition + selectedNodeId
ProcessPropertyPanel (右侧)
  ├─ selectedNodeId 解析 → phase / link / global
  ├─ LinkPropertyForm → updateLinkField → onDefinitionChange
  ├─ PhasePropertyForm → updatePhaseField → onDefinitionChange
  └─ GlobalPropertyForm → updateGlobalField → onDefinitionChange
```

**M4 不做的数据流**（留给 M5）：
- 可视化操作 → `yaml.dump` 刷新 Monaco
- Monaco 编辑 YAML → `parseYaml` → 更新 `definition` → React Flow 重渲染
- 保存按钮 → `PUT /api/v1/processes/{name}`

### 4.2 关键状态转换

| 转换 | 触发 | 副作用 |
|------|------|--------|
| `definition` 变更 | 可视化操作 / 属性面板字段修改 | `buildProcessGraph` 重新构建 nodes + edges，React Flow 重渲染 |
| `selectedNodeId` 变更 | 点击节点 / 点击画布空白 | 属性面板切换到对应表单 |
| `onConnect` | 拖拽连线 | `setLinkGoto` 更新 `on_success` / `on_gate_fail` → `onDefinitionChange` |
| `onNodesDelete` | 删除 link 节点 | `removeLink` 级联重置悬空 goto → `onDefinitionChange` |

---

## 5. 验收标准

### 5.1 功能验收

| 编号 | 验收项 |
|------|--------|
| AC-M4-1 | 进入编辑器后，可视化区显示泳道编辑器，可新增/删除 phase |
| AC-M4-2 | 每个 phase 内可新增/删除 link |
| AC-M4-3 | 点击 link 节点，右侧属性面板显示 8 个常用字段 + gates/expected_artifacts 嵌套表格 |
| AC-M4-4 | 点击 phase 节点，右侧属性面板显示 6 个字段 |
| AC-M4-5 | 拖拽连线：从绿色 handle 拖出更新 `on_success: goto:<target>` |
| AC-M4-6 | 拖拽连线：从橙色 handle 拖出更新 `on_gate_fail: goto:<target>` |
| AC-M4-7 | hover 连线显示删除按钮，点击删除后对应字段重置为 `next` / `break` |
| AC-M4-8 | 删除 link 节点：弹 `Modal.confirm`，确认后级联重置悬空 goto |
| AC-M4-9 | 删除 phase 容器：弹 `Modal.confirm`，确认后级联重置悬空 goto |
| AC-M4-10 | React Flow 画布支持 pan/zoom/MiniMap/Controls |
| AC-M4-11 | 新增 phase 时自动 `fitView` 到新节点区域 |
| AC-M4-12 | 属性面板修改字段后，可视化区实时更新 |

### 5.2 编译与测试验收

| 编号 | 验收项 |
|------|--------|
| AC-M4-V1 | `cd frontend && npx tsc --noEmit` 零错误 |
| AC-M4-V2 | `cd frontend && npm run build` 零新告警 |
| AC-M4-V3 | `processLayout` / `processGraphBuilder` / `processDefinitionUpdater` 有 vitest 单元测试且通过 |

---

## 6. 实施顺序

| 步骤 | 动作 | 验证点 |
|------|------|--------|
| 1 | 安装 `@xyflow/react` 依赖 | `package.json` 出现新依赖 |
| 2 | 新建 `processDefinitionUpdater.ts` + vitest 测试 | `npm test` 通过 |
| 3 | 新建 `processLayout.ts` + vitest 测试 | `npm test` 通过 |
| 4 | 新建 `processGraphBuilder.ts` + vitest 测试 | `npm test` 通过 |
| 5 | 新建 `PhaseNode.tsx` + `LinkNode.tsx` + `ProcessEdge.tsx` | `tsc --noEmit` 通过 |
| 6 | 新建 `ProcessVisualEditor.tsx`（React Flow 主画布） | `tsc --noEmit` 通过 |
| 7 | 新建 `LinkPropertyForm` + `PhasePropertyForm` + `GlobalPropertyForm` | `tsc --noEmit` 通过 |
| 8 | 新建 `ProcessPropertyPanel.tsx`（属性面板路由） | `tsc --noEmit` 通过 |
| 9 | 修改 `ProcessEditor.tsx`，扩展为 M4 双栏布局 | `tsc --noEmit` 通过 |
| 10 | `npm run build` 验证零告警 | build 成功 |
| 11 | `make dev` + Playwright 手动验证 AC-M4-1 ~ AC-M4-12 | 全部通过 |

---

## 7. 风险与缓解

| 风险 | 缓解措施 |
|------|---------|
| React Flow v12 的 `parentNode` / `Handle` API 与 v11 差异 | 查官方文档，用 `@xyflow/react` ^12.3.0 的类型签名 |
| 节点 id 用数组索引生成，增删时索引变化导致 React Flow 重渲染 | M4 接受这一限制（YAGNI），M5 可考虑用 `phase.id` / `link.id` 作为节点 id |
| 删除 phase 级联重置悬空 goto 引用的复杂度 | 用 `processDefinitionUpdater.ts` 纯函数封装，单元测试覆盖 |
| 属性面板嵌套表格（gates/expected_artifacts）的行内编辑复杂度 | 用 Ant Design Table 的 `components` 自定义单元格 |
| React Flow 包体积（~200KB gzip） | `React.lazy` 动态 import（与 Monaco 一样） |

---

## 8. 与需求的对应关系

| 需求条目 | M4 实现 | 状态 |
|---------|---------|------|
| 需求 §3.3 前端：可视化泳道编辑器（React Flow） | `ProcessVisualEditor` + `PhaseNode` + `LinkNode` + `ProcessEdge` | ✅ M4 |
| 需求 §3.3.2 节点模型 | `PhaseNode` (group) + `LinkNode` (子节点) | ✅ M4 |
| 需求 §3.3.3 连线模型 | 双 source handle + `onConnect` + 自定义 edge hover 删除 | ✅ M4 |
| 需求 §3.3.4 节点选中与属性面板 | `ProcessPropertyPanel` + `LinkPropertyForm` / `PhasePropertyForm` / `GlobalPropertyForm` | ✅ M4 |
| 需求 §3.3.5 全局字段面板 | `GlobalPropertyForm`（顶部折叠面板 + limits/abnormal_handler/step_templates） | ✅ M4 |
| 需求 §3.3.6 删除节点级联处理 | `removeLink` / `removePhase` 级联重置悬空 goto + `Modal.confirm` | ✅ M4 |
| 需求 §3.4 YAML ↔ 可视化双向联动 | sync flag | ⏳ M5 |
| 需求 §3.6 未保存修改离开拦截 | useBlocker + beforeunload | ⏳ M5 |
| 需求 §3.7 保存反馈 | message.success + isDirty | ⏳ M5 |
| 需求 §3.8 空工艺渲染 | Empty + CTA | ⏳ M6 |

---

## 9. 下一步

方案确认后，按 §6 实施顺序执行 M4。M4 完成后提交 commit，进入 M5（双向联动与保存）。
