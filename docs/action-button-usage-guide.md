# ActionButton 使用指南（AI Agent 参考）

## 概述

`ActionButton` 是一个可复用的 React 组件，用于在 ntd 的任意页面中添加「一键 AI 执行」能力。点击按钮后弹出 Drawer 面板，用户可编辑 Prompt、选择执行器，然后执行并应用结果。

## 快速开始

```tsx
import { ActionButton } from '@/components/ActionButton';
import { RocketOutlined } from '@ant-design/icons';

<ActionButton
  actionType="your_action_type"
  actionKey="default"
  prompt="你的 Prompt 模板，支持 {{key}} 占位符"
  params={{ key: "value" }}
  onApply={(result) => console.log(result)}
/>
```

## 属性说明

| 属性 | 类型 | 必填 | 默认值 | 说明 |
|------|------|------|--------|------|
| `actionType` | `string` | ✅ | - | 动作类型，用于查找/创建 todo |
| `actionKey` | `string` | ✅ | - | 动作键值，与 actionType 配合唯一标识 |
| `prompt` | `string` | ✅ | - | Prompt 模板，支持 `{{key}}` 占位符 |
| `params` | `Record<string, string>` | ✅ | - | 模板参数，替换 `{{key}}` |
| `onApply` | `(result: string) => void \| Promise<void>` | ✅ | - | 应用结果的回调 |
| `workspaceId` | `number` | ❌ | 第一个可用工作空间 | 工作空间 ID |
| `children` | `ReactNode` | ❌ | '智能执行' | 按钮文本 |
| `buttonType` | `'primary' \| 'default' \| 'link' \| 'text'` | ❌ | 'default' | 按钮类型 |
| `icon` | `ReactNode` | ❌ | `<ThunderboltOutlined />` | 按钮图标 |
| `disabled` | `boolean` | ❌ | false | 是否禁用 |
| `panelTitle` | `string` | ❌ | '自动优化标题' | Drawer 标题 |
| `panelDescription` | `string` | ❌ | '检查并确认...' | 面板描述 |
| `executor` | `string` | ❌ | 'claudecode' | 默认执行器 |

## Prompt 模板语法

使用 `{{key}}` 占位符，key 对应 `params` 中的键：

```tsx
prompt={`你是一个标题优化专家。

当前标题：{{title}}
当前 Prompt：{{prompt}}

请直接输出优化后的标题。`}
params={{
  title: "fix bug",
  prompt: "帮我修复登录超时的问题",
}}
```

## 输出格式约定

建议在 prompt 中要求 AI 使用 `RESULT` 标记包裹输出，便于精确提取：

```
输出格式：用 RESULT 标记包裹最终结果，不要加任何其他内容。

RESULT
你的输出内容
RESULT
```

前端可使用 `extractTitle()` 函数提取：

```tsx
import { extractTitle } from '@/utils/titleExtractor';

const title = extractTitle(aiResult); // 从 RESULT 标记中提取
```

## 常见使用场景

### 1. 标题优化

```tsx
<ActionButton
  actionType="title_optimize"
  actionKey="default"
  prompt={`你是一个标题优化专家。请根据以下信息生成更优的标题。

当前标题：{{title}}
当前 Prompt：{{prompt}}

要求：
1. 保持原意
2. 更简洁有力

输出格式：用 RESULT 标记包裹最终标题。

RESULT
优化后的标题
RESULT`}
  params={{ title: todo.title, prompt: todo.prompt || '' }}
  onApply={(newTitle) => updateTodo({ title: newTitle })}
  buttonType="text"
  icon={<RocketOutlined />}
  panelTitle="自动优化标题"
/>
```

### 2. Prompt 优化

```tsx
<ActionButton
  actionType="prompt_optimize"
  actionKey="default"
  prompt={`你是一个 Prompt 工程专家。请优化以下 Prompt。

原始 Prompt：{{prompt}}

要求：
1. 更清晰具体
2. 添加输出格式说明

RESULT
优化后的 Prompt
RESULT`}
  params={{ prompt: todo.prompt }}
  onApply={(newPrompt) => updateTodo({ prompt: newPrompt })}
/>
```

### 3. 内容摘要

```tsx
<ActionButton
  actionType="summarize"
  actionKey="default"
  prompt={`请为以下内容生成摘要：{{content}}`}
  params={{ content: longText }}
  onApply={(summary) => setSummary(summary)}
/>
```

## 后端工作原理

1. 前端调用 `POST /api/actions/execute`
2. 后端按 `action_type + action_key` 查找 todo
3. 找不到 → 自动创建 todo（prompt 来自请求）
4. 执行 todo，返回 `task_id` + `record_id`
5. 前端通过 WebSocket 监听执行完成
6. 获取结果并展示

## 注意事项

1. **action_type + action_key 组合必须唯一**，数据库有唯一索引
2. **prompt 中的 `{{key}}` 必须在 params 中有对应值**，否则会保留原始占位符
3. **面板在执行中无法关闭**，防止用户误关丢失结果
4. **左上角 X 按钮可关闭**，但遮罩点击始终禁用
5. **执行器可覆盖**，用户在面板中选择的执行器会覆盖 todo 默认值

## 文件位置

- 组件：`frontend/src/components/ActionButton/`
- 标题提取：`frontend/src/utils/titleExtractor.ts`
- 后端 API：`backend/src/handlers/action.rs`
- 数据库迁移：`backend/src/db/migration.rs` (V45/V46)
