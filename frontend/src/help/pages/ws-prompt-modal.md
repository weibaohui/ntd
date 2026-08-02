# 基础约定弹窗

## 功能位置
工作空间页 → 工作空间卡底部署理区 →「基础约定」可点击文本（`FileTextOutlined`）

## 数据流图（前端 → 后端）

```mermaid
flowchart LR
  User["点击基础约定"] --> setPromptModalWorkspace["setPromptModalWorkspace<br>{id, name}"]
  setPromptModalWorkspace --> WorkspacePromptModal["WorkspacePromptModal open"]
  WorkspacePromptModal -->|"useEffect open<br>db.getWorkspaceSettings"| API1["GET /api/v1/workspaces/{id}/settings"]
  API1 --> formSet["form.setFieldsValue<br>system_prompt"]
  WorkspacePromptModal -->|"handleSave<br>db.updateWorkspaceSettings"| API2["PUT /api/v1/workspaces/{id}/settings"]
```

## 谑用关系链路图

```mermaid
flowchart TD
  Panel["ProjectDirectoriesPanel.tsx<br>ProjectDirectoriesPanel()"] --> promptModalWorkspaceState["useState promptModalWorkspace<br>{id, name} | null"]
  Panel --> ClickLabel["基础约定 span onClick"]
  ClickLabel --> setPromptModalWorkspace["setPromptModalWorkspace<br>{id: dir.id, name: dir.name}"]
  Panel --> WorkspacePromptModal["workspace/WorkspacePromptModal.tsx"]
  WorkspacePromptModal --> useEffectOpen["useEffect open+workspaceId<br>db.getWorkspaceSettings"]
  useEffectOpen --> formSet["form.setFieldsValue<br>system_prompt"]
  WorkspacePromptModal --> handleSave["handleSave"]
  handleSave --> formValidate["form.validateFields"]
  formValidate --> db1["db.updateWorkspaceSettings(workspaceId, {system_prompt})"]
  db1 --> onSaved["onSaved → loadProjectDirectories"]
  db1 --> onClose["onClose"]
```

## 数据结构图

```mermaid
classDiagram
  class WorkspaceSettings {
    workspace_id: number
    system_prompt: string|null
    default_response_type: string
    default_response_todo_id: number|null
    default_response_loop_id: number|null
  }
```

## 数据变更图

```mermaid
stateDiagram-v2
  [*] --> Closed: 弹窗未打开
  Closed --> Loading: 点击基础约定
  Loading --> Open: getWorkspaceSettings 完成
  Open --> Saving: 点击确定 handleSave
  Saving --> Closed: 保存成功 onSaved+onClose
  Open --> Closed: 点击取消/关闭
```

## 开发指导
- **前端入口**：`frontend/src/components/settings/workspace/WorkspacePromptModal.tsx` 的 `WorkspacePromptModal` 组件；由 `ProjectDirectoriesPanel` 的 `promptModalWorkspace` state 驱动 `open`
- **后端入口**：`backend/src/handlers/` 对应 workspace settings handler 处理 `GET /api/v1/workspaces/{id}/settings`、`PUT /api/v1/workspaces/{id}/settings`
- **注意**：`system_prompt` 为 null 时视作空串显示；空串表示显式清空，不是「不保存」；内容会作为执行器前置 prompt 注入到该工作空间下所有 todo 的执行中
- **扩展**：如需为工作空间追加更多设置字段（如默认执行器），`WorkspaceSettings` 类型扩展并在弹窗中追加 Form.Item
