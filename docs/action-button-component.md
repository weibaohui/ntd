# Action Button Component — 可复用的一键 AI 执行组件

## 背景

在 ntd 的多个页面中，存在「选中文本 → 用 AI 优化 → 应用结果」的重复模式。例如：
- Todo 标题重写
- Prompt 优化
- 验收标准生成
- 内容摘要

目前每次实现这类功能都需要手动编写：创建 todo → 执行 → 等待结果 → 提取结论 → 应用/拒绝。逻辑高度重复，且缺乏统一的 UI 交互模式。

## 目标

封装一个**前后端配合的可复用组件**，在任意页面中一行代码即可接入「一键 AI 执行」能力。

### 核心交互流程

```
用户点击按钮
  → 弹出执行面板（Drawer）
  → 展示可编辑的 Prompt、执行器选择器、参数预览
  → 用户修改后点击「执行」
  → 后端用 action_type + action_key 查找或自动创建 todo
  → WebSocket 监听执行完成
  → 提取执行结论，展示结果
  → 用户选择「应用」或「拒绝」
    → 应用：调用 onApply 回调，由页面决定如何使用结果
    → 拒弃：关闭面板，不做任何修改
```

## 架构设计

### 后端：Action Execution API

新增 `POST /api/actions/execute` 端点，复用现有 `executor_service` 执行体系。

#### `POST /api/actions/execute` — 启动执行

**请求体：**

```json
{
  "action_type": "title_optimize",
  "action_key": "default",
  "prompt": "你是一个标题优化专家。请优化以下标题：\n\n{{title}}",
  "params": { "title": "fix bug" },
  "workspace_id": 1,
  "executor": "claudecode"
}
```

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `action_type` | string | ✅ | 动作类型（如 "title_optimize"） |
| `action_key` | string | ✅ | 动作键值（如 "default"） |
| `prompt` | string | ✅ | Prompt 模板，支持 `{{key}}` 占位符 |
| `params` | object | ✅ | 模板参数，替换 prompt 中的 `{{key}}` |
| `workspace_id` | number | ❌ | 工作空间 ID（不传则使用第一个可用工作空间） |
| `executor` | string | ❌ | 执行器类型（覆盖 todo 默认的 executor） |

**响应体：**

```json
{
  "code": 0,
  "data": {
    "task_id": "uuid-xxx",
    "record_id": 456,
    "todo_id": 123,
    "todo_created": false
  }
}
```

**后端逻辑：**

1. 参数校验：`action_type`、`action_key`、`prompt` 不能为空
2. 按 `action_type + action_key` 精确查找 todo
3. 找不到 → 自动创建 todo（prompt 来自请求）
4. 执行 todo，使用请求中指定的执行器（覆盖 todo 默认值）
5. 返回 `task_id` + `record_id` + `todo_id`

**数据库约束：**

- `action_type + action_key` 有唯一索引，防止并发重复创建

### 前端：ActionButton 组件

#### 组件接口

```typescript
interface ActionButtonProps {
  /** 动作类型（如 "title_optimize"、"prompt_optimize"） */
  actionType: string;
  /** 动作键值（如 "default"、"aggressive"） */
  actionKey: string;
  /** Prompt 模板，支持 {{key}} 占位符 */
  prompt: string;
  /** 模板参数 */
  params: Record<string, string>;
  /** 执行完成后「应用」的回调 */
  onApply: (result: string) => void | Promise<void>;
  /** 工作空间 ID（可选） */
  workspaceId?: number;
  /** 按钮显示内容 */
  children?: React.ReactNode;
  /** 按钮类型 */
  buttonType?: 'primary' | 'default' | 'link' | 'text';
  /** 按钮图标 */
  icon?: React.ReactNode;
  /** 是否禁用 */
  disabled?: boolean;
  /** 面板标题（默认：自动优化标题） */
  panelTitle?: string;
  /** 面板描述 */
  panelDescription?: string;
  /** 默认执行器类型 */
  executor?: string;
}
```

#### 组件状态机

```
IDLE → EXECUTING → COMPLETED → (APPLIED | REJECTED)
         ↓
       FAILED
```

#### 组件行为

1. **IDLE 状态**：点击按钮打开 Drawer，展示：
   - 可编辑的 Prompt TextArea
   - 执行器选择器（复用 ExecutorPicker 组件）
   - 参数预览（Tag + 多行文本）

2. **EXECUTING 状态**：Loading 状态，禁止关闭面板

3. **COMPLETED 状态**：展示 AI 生成结果

4. **FAILED 状态**：展示错误信息，支持重试

#### 使用示例

```tsx
import { ActionButton } from '@/components/ActionButton';
import { Tooltip } from 'antd';
import { RocketOutlined } from '@ant-design/icons';

// 在 Todo 标题行（图标按钮 + Tooltip）
<Tooltip title="自动优化标题">
  <ActionButton
    actionType="title_optimize"
    actionKey="default"
    prompt={`你是一个标题优化专家。请根据以下信息生成更优的标题。

当前标题：{{title}}
当前 Prompt：{{prompt}}

要求：
1. 保持原意
2. 更简洁有力
3. 适合 AI Todo 应用的场景

输出格式：用 RESULT 标记包裹最终标题。

RESULT
优化后的标题文本
RESULT`}
    params={{
      title: selectedTodo.title,
      prompt: selectedTodo.prompt || '',
    }}
    workspaceId={selectedTodo.workspace_id}
    onApply={handleTitleUpdate}
    buttonType="text"
    icon={<RocketOutlined />}
    panelTitle="自动优化标题"
  />
</Tooltip>
```

## 文件结构

```
backend/src/
  handlers/action.rs          # POST /api/actions/execute
  db/todo.rs                  # get_todo_by_action_type_and_key()
  db/migration.rs             # V45/V46: action_type + action_key 列 + 唯一索引
  db/entity/todos.rs          # 实体字段
  models/mod.rs               # Todo/CreateTodoRequest/UpdateTodoRequest

frontend/src/
  components/ActionButton/
    index.tsx                 # 主组件（Button + Drawer）
    types.ts                  # 类型定义
    useActionExecution.ts     # 执行状态管理 + WebSocket 监听
  utils/titleExtractor.ts     # 从 AI 结果中提取标题
  components/todo-detail/DetailHeader.tsx  # 集成示例
```

## 配置与扩展

### 如何新增一种 Action

无需修改后端代码，只需：

1. 在前端定义 prompt 模板和 params
2. 使用 `<ActionButton>` 组件，传入 `actionType` + `actionKey`

后端会自动查找或创建对应的 todo。

## 验收标准

- [x] 后端：`POST /api/actions/execute` 能正确查找/创建 todo 并执行
- [x] 后端：参数校验、唯一索引
- [x] 前端：ActionButton 组件在 PC 和移动端正常渲染
- [x] 前端：可编辑 Prompt、执行器选择器、参数预览
- [x] 前端：应用/拒绝按钮正常工作
- [x] 前端：暗色主题适配
- [x] 后端单元测试通过
- [x] 前端 TypeScript 编译通过
