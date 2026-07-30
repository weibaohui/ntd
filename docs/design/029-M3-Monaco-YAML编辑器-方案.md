# 029-M3 Monaco YAML 编辑器 — 方案文档

| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| AtomCode (GLM-5.2) | 2026-07-27 | 初始版本，M3 里程碑实施方案 |

---

## 1. 背景与定位

本方案是 029 工艺编辑器的第 3 个里程碑（M3），对应：
- 需求文档：`docs/requirements/029-工艺模板编辑与可视化创建-需求.md` §3.2（YAML 编辑器）
- 设计文档：`docs/design/029-工艺模板编辑与可视化创建-设计.md` §6（Monaco YAML 编辑器）

### 1.1 里程碑进度

| 里程碑 | 状态 | 说明 |
|--------|------|------|
| M1 后端 API | ✅ commit a14da299 | PUT/POST/DELETE + 13 单元测试 |
| M2 前端路由与入口 | ✅ commit d44b6a30 | `useViewState` 支持 `processEditor`，`ProcessPage` 有 `ProcessEditorPlaceholder` 占位 |
| **M3 Monaco YAML 编辑器** | **⏳ 本方案** | Monaco 集成 + js-yaml 实时解析 + 错误标记 |
| M4 React Flow 可视化 | 待做 | 泳道编辑器 |
| M5 双向联动与保存 | 待做 | sync flag + 保存/删除按钮 |
| M6 新建工艺流程 | 待做 | 元信息 Modal + 空工艺渲染 |
| M7 编译告警清理与测试 | 待做 | 全量验证 |

### 1.2 M3 的边界

**做**：
1. 安装 Monaco 依赖
2. 实现 `ProcessYamlEditor.tsx`（Monaco 封装 + 主题跟随 + 只读控制）
3. 实现 `processYamlValidator.ts`（js-yaml 解析 + 错误行号/消息提取，纯函数易测）
4. 用真实编辑器替换 `ProcessEditorPlaceholder`，接入 M2 路由
5. 系统工艺只读 + 黄色 Alert + "复制到用户层"链接
6. 用户工艺可编辑 + 绿色 Alert

**不做**（留给后续里程碑）：
- React Flow 可视化（M4）
- 双向联动 sync flag（M5）— M3 编辑器只做 YAML 文本编辑 + 实时校验标记，不回写 `ProcessDefinition` 对象
- 保存/删除按钮调用后端（M5）— M3 只渲染编辑器，保存按钮留 M5 接
- 新建工艺元信息 Modal（M6）— M3 处理 `mode: 'edit'`，`mode: 'new'` 留 M6
- Monaco 的 JSON Schema 自动补全（YAGNI，需求 §2.2 明确不做）

### 1.3 关键现状核对

| 项 | 现状 | M3 处理 |
|----|------|---------|
| `js-yaml` / `@types/js-yaml` | ✅ 已在 `package.json`（`^4.1.1` / `^4.0.9`） | 直接复用，不重复安装 |
| `dagre` / `@types/dagre` | ✅ 已在 `package.json`（M4 用） | M3 不动 |
| Monaco 依赖 | ❌ 未安装 | M3 新增 `@monaco-editor/react` + `monaco-editor` |
| `bundled.ts` API 客户端 | ✅ 已有 `getProcess` / `copyProcessToUser` | M3 直接调用，不扩展 |
| `ProcessEditorPlaceholder` | 占位组件，在 `ProcessPage.tsx` L77-88 | M3 替换为真实编辑器 |
| `useViewState` 的 `processEditor` 视图 | ✅ M2 已实现路由解析 | M3 消费 `processName` / `processMode` |

---

## 2. 技术决策

### 2.1 Monaco 加载策略：按需动态 import

**决策**：用 `React.lazy(() => import('@monaco-editor/react'))` 包裹 `ProcessYamlEditor`，只在进入 `/#/processes/{name}/edit` 路由时加载。

**理由**：
- Monaco 主包 ~2MB，全量加载会拖慢列表页首屏
- `@monaco-editor/react` 自带 loader 配置，默认从 CDN 加载 worker，但本方案改为本地 bundle（离线可用 + 版本锁定）

**worker 配置**：
- 在 `ProcessYamlEditor.tsx` 顶部通过 `loader.config({ monaco })` 注入本地 `monaco-editor`
- Vite 会自动处理 worker 分包，无需手动 `?worker` 导入

### 2.2 错误标记策略：行号槽标红 + 浮窗

**决策**：用 Monaco 的 `deltaDecorations` API 在行号槽（glyph margin）标红波浪线，hover 显示 `js-yaml` 抛出的错误消息。

**实现要点**：
- `js-yaml` 的 `YAMLException` 自带 `mark.line`（0-based）和 `message`（含行号）
- 转换为 1-based 行号供 Monaco `Range` 使用
- 错误标记随每次 `onChange`（debounced 300ms）更新
- 多个错误只标第一个（YAML 解析遇错即停，不会产生多个错误）

### 2.3 主题跟随：跟随应用暗/亮模式

**决策**：通过 `useTheme()` hook 获取当前主题，映射到 Monaco 主题：
- `dark` → `vs-dark`
- `light` → `vs`

**理由**：Monaco 自带这两套主题，无需自定义。`useTheme()` 是项目既有 hook（见 `@/hooks/useTheme`）。

### 2.4 系统工艺只读 + 复制链接

**决策**：
- `is_system === true` → Monaco `readOnly: true` + 顶部黄色 Alert
- Alert 内嵌"复制到用户层后编辑"链接按钮
- 点击调用 `bundledApi.copyProcessToUser(name)`，成功后 `message.success` 并提示用户重新打开编辑器（M3 不自动跳转，避免复杂状态处理）

**理由**：系统工艺被编辑后会被 `git reset --hard` 覆盖（需求 §1.2），必须先复制到用户层。

### 2.5 M3 编辑器组件结构（不含 M4/M5）

```
ProcessPage (M2 已接入路由)
└── mode === 'edit' ? <ProcessEditor> : <ProcessEditorPlaceholder>  (M6 处理 new)
    └── ProcessEditor (M3 骨架，M4/M5 会扩展)
        ├── 顶部 Alert（系统/用户工艺提示）
        └── ProcessYamlEditor (Monaco + js-yaml 校验)
```

**M3 的 `ProcessEditor` 是骨架**：
- 加载工艺详情（`bundledApi.getProcess(name)`）
- 渲染 Alert + Monaco 编辑器
- **不实现保存按钮**（留 M5）
- **不实现可视化区**（留 M4）
- **不实现离开拦截**（留 M5）

---

## 3. 文件改动清单

### 3.1 新增文件

#### 3.1.1 `frontend/src/components/process/processYamlValidator.ts`

**职责**：纯函数，封装 js-yaml 解析 + 错误提取。

**接口**：
```typescript
// YAML 解析结果
export interface YamlParseResult {
  // 解析成功时为解析后的对象，失败时为 null
  parsed: unknown | null;
  // 解析失败时的错误信息，成功时为 null
  error: YamlError | null;
}

// YAML 错误信息（供 Monaco 标记用）
export interface YamlError {
  // 1-based 行号（js-yaml 的 mark.line 是 0-based）
  line: number;
  // 错误消息（含原始行号信息）
  message: string;
}

// 解析 YAML 文本，返回成功对象或错误信息
export function parseYaml(yamlText: string): YamlParseResult;
```

**实现要点**：
- `yaml.load(yamlText, { schema: yaml.DEFAULT_FULL_SCHEMA })` 解析
- `try/catch` 捕获 `YAMLException`
- 从 `err.mark.line`（0-based）转 1-based
- 空字符串或纯空白返回 `{ parsed: null, error: null }`（不报错，视为空工艺）

**单元测试**（vitest）：
- `parseYaml_validYaml_returnsParsedObject`：合法 YAML 返回对象
- `parseYaml_emptyString_returnsNullParsedNoError`：空串不报错
- `parseYaml_syntaxError_returnsErrorWithLine`：语法错误返回行号
- `parseYaml_tabIndentError_returnsErrorWithLine`：Tab 缩进错误返回行号

#### 3.1.2 `frontend/src/components/process/ProcessYamlEditor.tsx`

**职责**：Monaco 编辑器封装 + 错误标记 + 只读控制。

**接口**：
```typescript
interface ProcessYamlEditorProps {
  // YAML 文本
  value: string;
  // 文本变化回调
  onChange: (newText: string) => void;
  // 是否只读（系统工艺 true）
  readOnly: boolean;
  // 主题（从 useTheme 传入，避免组件内重复 hook）
  theme: 'dark' | 'light';
}
```

**实现要点**：
- 用 `@monaco-editor/react` 的 `Editor` 组件
- `onMount` 保存 editor 引用
- `useEffect` 监听 `value` 变化，调用 `parseYaml` 校验
- 校验失败时用 `deltaDecorations` 在 `error.line` 标红
- 主题映射：`dark` → `vs-dark`，`light` → `vs`
- options：`minimap: false`、`fontSize: 13`、`lineNumbers: 'on'`、`glyphMargin: true`、`scrollBeyondLastLine: false`、`readOnly`

**错误标记 CSS**（在组件内用 `<style>` 标签注入，避免新增全局 CSS 文件）：
```css
.yaml-error-line { background: rgba(248, 81, 73, 0.15); }
.yaml-error-glyph { background: #f85149; border-radius: 3px; color: #fff; }
```

#### 3.1.3 `frontend/src/components/process/ProcessEditor.tsx`

**职责**：M3 编辑器骨架，加载工艺 + 渲染 Alert + Monaco。

**接口**：
```typescript
interface ProcessEditorProps {
  // 工艺名（从路由参数取）
  processName: string;
  // 工作空间 ID（getProcess 需要）
  workspaceId?: number;
  // 复制到用户层后的回调（M3 用于刷新状态，M5 会用于保存后刷新）
  onCopiedToUser?: () => void;
}
```

**实现要点**：
- `useState` 管理 `detail`（`ProcessTemplateDetail | null`）、`loading`、`yamlText`、`isSystem`
- `useEffect` 加载 `bundledApi.getProcess(processName)`，提取 `definition` 和 `is_system`
- 顶部 Alert：系统工艺黄色 + 复制链接，用户工艺绿色
- Monaco 编辑器：`value={yamlText}`、`onChange={setYamlText}`、`readOnly={isSystem}`
- **M3 不实现保存按钮**（留 M5），只做编辑 + 实时校验
- **M3 不实现离开拦截**（留 M5）

**加载状态**：
- `loading` → `<Spin tip="加载工艺..." />`
- `detail === null` 且非 loading → `<Empty description="工艺不存在" />`
- `detail` 存在 → Alert + Monaco

### 3.2 修改文件

#### 3.2.1 `frontend/package.json`

新增依赖：
```json
{
  "@monaco-editor/react": "^4.6.0",
  "monaco-editor": "^0.50.0"
}
```

**安装方式**：`cd frontend && npm install @monaco-editor/react@^4.6.0 monaco-editor@^0.50.0`

**理由**：
- `@monaco-editor/react` 是 React 官方封装，处理了 worker 加载和生命周期
- `monaco-editor` 是 Monaco 本体，`@monaco-editor/react` 4.x 依赖它
- 版本选择最新稳定版（4.6.0 / 0.50.0）

#### 3.2.2 `frontend/src/components/ProcessPage.tsx`

**改动点**：替换 `ProcessEditorPlaceholder`（L77-88）为真实 `ProcessEditor`。

**当前代码**（L77-88）：
```tsx
function ProcessEditorPlaceholder({ mode, name }: { mode: 'new' | 'edit'; name: string | null }) {
  return (
    <div style={{ padding: 60, textAlign: 'center', color: '#94a3b8' }}>
      <Title level={4}>
        {mode === 'new' ? '创建新工艺（编辑器开发中）' : `编辑工艺：${name ?? '未知'}（编辑器开发中）`}
      </Title>
      <Text type="secondary">
        029 工艺编辑器正在开发中（M3-M6 阶段填充 Monaco YAML 编辑器 + React Flow 泳道可视化）。
      </Text>
    </div>
  );
}
```

**M3 改动**：
- `mode === 'edit'` 且 `name` 存在 → 渲染 `<ProcessEditor processName={name} />`
- `mode === 'new'` → 保留占位（M6 实现）
- `mode === 'edit'` 但 `name` 为 null → 渲染错误提示"缺少工艺名"

**调用位置**：`ProcessPage` 函数体（L50-64）里根据 `processMode` 分流，当前是 `processMode === 'list' ? <ProcessListView> : <ProcessEditorPlaceholder>`，改为：
```tsx
processMode === 'list' ? <ProcessListView ... /> : <ProcessEditorRouter mode={processMode} name={processName} />
```

其中 `ProcessEditorRouter` 是 M3 新增的小组件，负责 `new`/`edit` 分流和参数校验。

### 3.3 不改动的文件（M3 范围外）

- `backend/` — M1 已完成，M3 不动后端
- `useViewState.ts` — M2 已实现路由解析，M3 不动
- `App.tsx` — M2 已接入 `processEditor` 视图，M3 不动

---

## 4. 数据流

### 4.1 M3 数据流（单向，只 YAML 编辑）

```
路由 /#/processes/{name}/edit
  ↓
useViewState 解析 → processMode='edit', processName='{name}'
  ↓
ProcessPage → ProcessEditorRouter → ProcessEditor
  ↓
ProcessEditor:
  1. useEffect → bundledApi.getProcess(name) → setDetail, setYamlText(detail.definition), setIsSystem(detail.is_system)
  2. 渲染 Alert（根据 isSystem）
  3. 渲染 ProcessYamlEditor:
     - value={yamlText}
     - onChange={setYamlText}
     - readOnly={isSystem}
  ↓
ProcessYamlEditor 内部:
  - Monaco 渲染 value
  - 用户编辑 → onChange 触发 → 父组件 setYamlText
  - value 变化 → useEffect 调用 parseYaml
  - parseYaml 失败 → deltaDecorations 标红
```

**M3 不做的数据流**（留给 M4/M5）：
- 可视化操作 → 修改 `ProcessDefinition` 对象 → `yaml.dump` 刷新 `yamlText`
- YAML 解析成功 → 更新 `ProcessDefinition` 对象 → React Flow 重渲染
- 保存按钮 → `PUT /api/v1/processes/{name}`

### 4.2 状态管理

M3 的 `ProcessEditor` 组件状态：

```typescript
// 工艺详情（含 definition YAML 文本）
const [detail, setDetail] = useState<ProcessTemplateDetail | null>(null);
// 加载中
const [loading, setLoading] = useState(true);
// 当前 YAML 文本（Monaco 编辑的内容）
const [yamlText, setYamlText] = useState('');
// 是否系统工艺
const [isSystem, setIsSystem] = useState(false);
```

**M3 不引入 `isDirty` / `isSyncing` / `ProcessDefinition` 对象**（留 M5）。

---

## 5. 验收标准

### 5.1 功能验收

| 编号 | 验收项 |
|------|--------|
| AC-M3-1 | 进入 `/#/processes/{name}/edit` 路由，加载工艺详情并渲染 Monaco 编辑器 |
| AC-M3-2 | Monaco 编辑器显示 YAML 语法高亮、行号、折叠 |
| AC-M3-3 | 用户工艺（`is_system=false`）的 Monaco 可编辑，顶部绿色 Alert |
| AC-M3-4 | 系统工艺（`is_system=true`）的 Monaco 只读，顶部黄色 Alert + "复制到用户层"链接 |
| AC-M3-5 | 输入非法 YAML 时，行号槽标红波浪线 + hover 显示错误消息 |
| AC-M3-6 | 主题切换时 Monaco 主题跟随（dark/light） |
| AC-M3-7 | `mode: 'new'` 仍显示占位（M6 实现） |

### 5.2 编译与测试验收

| 编号 | 验收项 |
|------|--------|
| AC-M3-V1 | `cd frontend && npx tsc --noEmit` 零错误 |
| AC-M3-V2 | `cd frontend && npm run build` 零新告警 |
| AC-M3-V3 | `processYamlValidator` 有 vitest 单元测试且通过 |
| AC-M3-V4 | Monaco 通过 `React.lazy` 动态加载，列表页首屏不加载 Monaco |

---

## 6. 实施顺序

| 步骤 | 动作 | 验证点 |
|------|------|--------|
| 1 | `cd frontend && npm install @monaco-editor/react@^4.6.0 monaco-editor@^0.50.0` | `package.json` 出现新依赖 |
| 2 | 新建 `processYamlValidator.ts` + vitest 测试 | `npm test` 通过 |
| 3 | 新建 `ProcessYamlEditor.tsx`（Monaco 封装 + 错误标记） | `tsc --noEmit` 通过 |
| 4 | 新建 `ProcessEditor.tsx`（M3 骨架：加载 + Alert + Monaco） | `tsc --noEmit` 通过 |
| 5 | 修改 `ProcessPage.tsx`，替换 `ProcessEditorPlaceholder` 为 `ProcessEditorRouter` | `tsc --noEmit` 通过 |
| 6 | `npm run build` 验证零告警 | build 成功 |
| 7 | `make dev` + Playwright 手动验证 AC-M3-1 ~ AC-M3-7 | 全部通过 |

---

## 7. 风险与缓解

| 风险 | 缓解措施 |
|------|---------|
| Monaco worker 在 Vite 下加载失败 | 用 `@monaco-editor/react` 的 `loader.config({ monaco })` 注入本地 `monaco-editor`，Vite 自动处理 worker 分包 |
| Monaco 包体积拖慢首屏 | `React.lazy` 动态 import，只在编辑器路由加载 |
| `js-yaml` 错误消息行号不准 | 单元测试覆盖 Tab 缩进、缺少空格、未闭合引号等场景，确认行号映射正确 |
| 系统工艺被误编辑后同步覆盖 | `readOnly: true` + 黄色 Alert + "复制到用户层"链接，三重防护 |
| 主题切换时 Monaco 不跟随 | `useTheme()` 监听主题变化，`useEffect` 重新设置 Monaco 主题 |

---

## 8. 与需求的对应关系

| 需求条目 | M3 实现 | 状态 |
|---------|---------|------|
| 需求 §3.2.1 依赖新增 | `package.json` 加 Monaco | ✅ M3 |
| 需求 §3.2.2 编辑器组件 | `ProcessYamlEditor.tsx` | ✅ M3 |
| 需求 §3.2.3 编辑权限控制 | Alert + readOnly | ✅ M3 |
| 需求 §3.3 可视化泳道编辑器 | React Flow | ⏳ M4 |
| 需求 §3.4 YAML ↔ 可视化双向联动 | sync flag | ⏳ M5 |
| 需求 §3.5 路由与入口 | M2 已完成 | ✅ M2 |
| 需求 §3.6 离开拦截 | useBlocker + beforeunload | ⏳ M5 |
| 需求 §3.7 保存反馈 | message.success + isDirty | ⏳ M5 |
| 需求 §3.8 空工艺渲染 | Empty + CTA | ⏳ M6 |

---

## 9. 下一步

方案确认后，按 §6 实施顺序执行 M3。M3 完成后提交 commit，进入 M4（React Flow 可视化编辑器）。
