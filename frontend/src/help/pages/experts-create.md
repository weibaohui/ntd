# AI 创建专家

## 功能位置
专家页 → 搜索栏右侧「AI 创建专家」按钮（`ExpertCreateModal`）

## 数据流图（前端 → 后端）

```mermaid
flowchart LR
  User["点击 AI 创建专家"] --> ActionButton["ActionButton Drawer"]
  ActionButton -->|"填写 description 模板"| Execute["执行 AI 生成"]
  Execute -->|"completedView"| ExpertCreateCompleted["ExpertCreateCompleted"]
  ExpertCreateCompleted -->|"handleCreate<br>db.createExpert"| API["POST /api/v1/experts/create"]
  API --> onCreated["onCreated → loadExperts"]
```

## 谑用关系链路图

```mermaid
flowchart TD
  ExpertCreateModal["ExpertCreateModal.tsx<br>ExpertCreateModal()"] --> useTodos["useTodos()<br>获取 selectedWorkspace"]
  useTodos --> ActionButton["ActionButton<br>actionType/key/prompt/executor"]
  ActionButton --> DrawerPanel["Drawer 展示模板输入"]
  DrawerPanel --> ExecuteState["executing 态<br>ChatView 日志流"]
  ExecuteState --> CompletedState["completed 态"]
  CompletedState --> ExpertCreateCompleted["ExpertCreateCompleted.tsx"]
  ExpertCreateCompleted --> parseResult["useMemo parseResult<br>正则提取 json/markdown 块"]
  parseResult --> pluginPreview["useMemo pluginPreview<br>JSON.parse"]
  ExpertCreateCompleted --> handleCreate["handleCreate"]
  handleCreate --> db1["db.createExpert(plugin_json, agent_md)"]
  handleCreate --> onCreated["onCreated() → loadExperts"]
```

## 数据结构图

```mermaid
classDiagram
  class ExpertCreateCompleted {
    result: string
    pluginJson: string
    agentMd: string
    creating: boolean
  }
  class ParseResult {
    pluginJson: string
    agentMd: string
    raw: boolean
  }
  ExpertCreateCompleted --> ParseResult: useMemo parseResult
```

## 数据变更图

```mermaid
stateDiagram-v2
  [*] --> DrawerOpen: 点击按钮
  DrawerOpen --> Executing: 点击执行
  Executing --> Completed: AI 输出完成
  Completed --> Creating: 点击 handleCreate
  Creating --> DrawerOpen: 创建成功 onCreated
  Creating --> Completed: 创建失败
```

## 开发指导
- **前端入口**：`frontend/src/components/settings/ExpertCreateModal.tsx` 的 `ExpertCreateModal` 组件，复用 `ActionButton` 交互流程；完成态由 `ExpertCreateCompleted.tsx` 渲染
- **后端入口**：`backend/src/handlers/experts.rs` 处理 `POST /api/v1/experts/create`，写入 `plugin.json` 和 `agent.md`
- **注意**：`parseResult` 用正则 `` ```json `` 和 `` ```markdown `` 提取 AI 输出块，若任一缺失则 `raw=true` 展示原始文本供用户手动修正
- **扩展**：新增 AI 模板字段时改 `EXPERT_CREATE_PROMPT` 和 `ExpertCreateCompleted` 的解析逻辑
