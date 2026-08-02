# 标签管理 Tab

## 功能位置
更多设置页 →「标签管理」Tab（`TagsPanel`）

## 数据流图（前端 → 后端）

```mermaid
flowchart LR
  User["输入名称+颜色<br>点击创建"] --> handleCreateTag["handleCreateTag"]
  handleCreateTag -->|"db.createTag(name, color)"| API1["POST /api/v1/tags"]
  API1 --> dispatch["dispatch ADD_TAG"]
  User["点击删除"] --> Popconfirm["Popconfirm"]
  Popconfirm --> handleDeleteTag["handleDeleteTag(tagId)"]
  handleDeleteTag -->|"db.deleteTag(tagId)"| API2["DELETE /api/v1/tags/{id}"]
  API2 --> dispatch2["dispatch DELETE_TAG"]
```

## 调用关系链路图

```mermaid
flowchart TD
  SettingsPage["SettingsPage.tsx<br>SettingsPage()"] --> TagsTab["Tab tags<br>tags + dispatch 传入"]
  TagsTab --> TagsPanel["TagsPanel.tsx<br>TagsPanel(tags, dispatch)"]
  TagsPanel --> tagNameState["useState tagName"]
  TagsPanel --> tagColorState["useState tagColor"]
  TagsPanel --> InputName["Input tagName<br>onPressEnter=handleCreateTag"]
  TagsPanel --> ColorPicker["ColorPicker tagColor"]
  TagsPanel --> CreateBtn["创建标签 Button<br>onClick=handleCreateTag"]
  CreateBtn --> handleCreateTag["handleCreateTag"]
  handleCreateTag --> trimCheck["name trim 非空"]
  trimCheck --> db1["db.createTag(name, color)"]
  db1 --> dispatch1["dispatch ADD_TAG payload=newTag"]
  db1 --> reset["setTagName('')<br>setTagColor('#0891b2')"]
  TagsPanel --> List["现有标签 List"]
  List --> Popconfirm["Popconfirm 删除确认"]
  Popconfirm --> handleDeleteTag["handleDeleteTag(tagId)"]
  handleDeleteTag --> db2["db.deleteTag(tagId)"]
  db2 --> dispatch2["dispatch DELETE_TAG payload=tagId"]
```

## 数据结构图

```mermaid
classDiagram
  class Tag {
    id: number
    name: string
    color: string
  }
```

## 数据变更图

```mermaid
stateDiagram-v2
  [*] --> Idle: Tab 加载
  Idle --> Creating: 点击创建
  Creating --> Idle: 创建成功 dispatch ADD_TAG
  Creating --> Idle: 创建失败
  Idle --> ConfirmOpen: 点击删除
  ConfirmOpen --> Idle: 取消 Popconfirm
  ConfirmOpen --> Deleting: 确认删除
  Deleting --> Idle: 删除成功 dispatch DELETE_TAG
```

## 开发指导
- **前端入口**：`frontend/src/components/settings/TagsPanel.tsx` 的 `TagsPanel` 组件；`tags` 和 `dispatch` 由 `SettingsPage` 从 `useApp` 传入
- **后端入口**：`backend/src/handlers/tag.rs` 处理 `POST /api/v1/tags`、`DELETE /api/v1/tags/{id}`、`GET /api/v1/tags`
- **注意**：创建成功后 `dispatch ADD_TAG` 让全局 state 同步；标签颜色默认 `#0891b2`，创建后重置色值
- **扩展**：如需标签分组或排序，`Tag` 类型扩展 `group` 字域，`TagsPanel` 的 `List` dataSource 按分组渲染
