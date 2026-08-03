# 编辑工艺

## 功能位置

工艺 → 用户工艺卡片 actions「编辑」按钮 或 详情 YAML Tab「编辑工艺」按钮 → 跳路由 `processes?processMode=edit&guid=<guid>` → `ProcessEditor` 三栏布局（工具栏 + 可视化/YAML 双 Tab + 属性面板）

## 数据流图（前端 → 后端）

```mermaid
flowchart LR
  U[用户点编辑] --> NAV["pushUrl processes?processMode=edit&guid"]
  NAV --> PP["ProcessPage 按 mode 分流"]
  PP --> ED["ProcessEditor(processGuid)"]
  ED --"getProcess(guid)"--> API1["/api/bundled/processes/{guid}"]
  API1 --> H1["get_process_template handler"]
  H1 --> FS["磁盘读 YAML 正文"]
  ED --"parseYaml(definition)"--> DEF["definition 状态 React Flow source of truth"]
  ED --"handleDefinitionChange"--> YD["yamlDump 刷新 Monaco"]
  ED --"handleYamlChange debounced parseYaml"--> DEF
  ED --"handleSave putProcess(guid, yamlText)"--> API2["PUT /api/v1/processes/{guid}"]
  API2 --> H2["update_process handler"]
  H2 --"is_system 校验 409"--> DB1[(process_templates 表)]
  H2 --"serde_yaml 校验 + guid 对齐 + version bump"--> YN["新 YAML 文本"]
  H2 --"atomic_write"--> FS2["写盘 ~/.ntd/processes/<rel>"]
  H2 --"snapshot_process_template_version"--> DB2[(process_template_versions 表)]
  H2 --"import_user_process_templates"--> DB3["刷新入库"]
  ED --"handleDelete deleteProcess(guid)"--> API3["DELETE /api/v1/processes/{guid}"]
```

## 调用关系链路图

```mermaid
flowchart TD
  ED["ProcessEditor useEffect([processGuid])"] --> LD["loadDetail"]
  LD --> API["bundledApi.getProcess(guid)"]
  API --> SET["setDetail / setYamlText / setDefinition"]
  HDC["handleDefinitionChange 可视化→YAML"] --> SD["setDefinition"]
  SD --> DIR["setIsDirty(true)"]
  SD --> SYNC["setIsSyncing(true) 避循环"]
  SD --> YD["yamlDump → setYamlText"]
  HYC["handleYamlChange YAML→可视化"] --> SYT["setYamlText"]
  SYT --> DIR2["setIsDirty(true)"]
  HYC --> DEB["debounced 300ms parseYaml"]
  DEB --> SD2["setDefinition 遱 React Flow 重渲"]
  HS["handleSave"] --> API2["bundledApi.putProcess(guid, yamlText)"]
  API2 --> PUT["PUT /api/v1/processes/{guid} body={definition}"]
  PUT --> H["backend update_process"]
  H --> GT["db.get_process_template_by_guid"]
  H --> SYS["if is_system → 409"]
  H --> PAR["serde_yaml::from_str ProcessDefinition"]
  H --> BUMP["bump_semver_minor 次版本递增"]
  H --> AW["atomic_write 写盘"]
  H --> SNAP["snapshot_process_template_version 版本快照"]
  H --> IMP["import_user_process_templates 刷新入库"]
```

## 数据结构图

```mermaid
classDiagram
  class ProcessEditorProps {
    +processGuid: string
  }
  class ProcessDefinition {
    +process: ProcessMeta
    +phases: PhaseDefinition[]
  }
  class UpdateProcessRequest {
    +definition: String
  }
  class ProcessTemplateDetail {
    +definition: String
    +is_system: bool
    +version: String
  }
  class ProcessTemplate {
    +guid: String
    +is_system: bool
    +version: String
    +source_path: Option~String~
  }
  ProcessEditorProps --> ProcessTemplateDetail: getProcess 加载
  ProcessTemplateDetail --> ProcessDefinition: parseYaml
  ProcessDefinition --> UpdateProcessRequest: yamlDump 成 definition
  UpdateProcessRequest --> ProcessTemplate: guid 寻址更新
```

## 数据变更图

```mermaid
stateDiagram-v2
  [*] --> 加载中
  加载中 --> 可视化Tab: getProcess 成功
  可视化Tab --> YAMLTab: 用户切 Tab
  YAMLTab --> 可视化Tab: 双向联动 debounced parseYaml
  可视化Tab --> 未保存: 属性面板改字段
  YAMLTab --> 未保存: Monaco 改文本
  未保存 --> 保存中: 点保存
  保存中 --> 可视化Tab: putProcess 成功 回刷 YAML + 清 isDirty
  保存中 --> 未保存: 失败 message.error
  未保存 --> 删除确认: 点删除
  删除确认 --> 已删除: Modal.confirm onOk
  已删除 --> 列表态: hash 跳 #/processes
  已删除 --> [*]
```

## 开发指导

- **前端入口**：`frontend/src/components/process/ProcessEditor.tsx` 的 `ProcessEditor` 组件；`handleDefinitionChange` / `handleYamlChange` / `handleSave` / `handleDelete`；`parseYaml` / `yamlDump` 在 `frontend/src/components/process/processYamlValidator.ts`
- **后端入口**：`backend/src/handlers/process.rs` 的 `update_process` / `delete_process` handler，路由 `PUT /api/v1/processes/{guid}` / `DELETE /api/v1/processes/{guid}`
- **注意**：系统工艺（`is_system=true`）后端返回 409，前端编辑器内给「复制为我的工艺」按钮而非直接编辑；保存后回刷 Monaco 为后端递增后的真值 YAML（含新 version），防陈旧 version 下次保存误判；`isSyncing` flag 防 Monaco/React Flow 双向循环
- **扩展**：新增工艺字段时，扩 `ProcessDefinition` / `ProcessMeta`、属性面板加对应表单、`yamlDump` 序列化键、后端 `ProcessDefinition` serde struct + installer 读取
