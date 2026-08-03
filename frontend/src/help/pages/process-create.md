# 创建工艺

## 功能位置

工艺 → 右上角「创建工艺」按钮 → 弹出 `CreateProcessMetaModal` 元信息表单 → 填写 6 字段确认 → 跳编辑器路由

## 数据流图（前端 → 后端）

```mermaid
flowchart LR
  U[用户点击创建工艺] --> MOD["CreateProcessMetaModal 表单"]
  MOD --"debounced getProcesses() 比对"--> API1["/api/bundled/processes"]
  MOD --"buildEmptyProcessYaml(meta)"--> YFN["构造空工艺 YAML 文本"]
  YFN --"postProcess(meta)"--> API2["POST /api/v1/processes"]
  API2 --> H["create_process handler"]
  H --"validate_process_name / 正则校验"--> V1["name 合法性校验"]
  H --"get_process_template_by_name"--> DB1[(process_templates 表)]
  H --"serde_yaml 结构校验"--> V2["YAML 解析"]
  H --"atomic_write"--> FS["磁盘 ~/.ntd/processes/<name>.yaml"]
  H --"import_user_process_templates"--> DB2["刷新入库为 is_system=false"]
  MOD --"onCreated(guid)"--> NAV["pushUrl processes?processMode=edit&guid"]
```

## 调用关系链路图

```mermaid
flowchart TD
  CM["CreateProcessMetaModal.handleSubmit"] --> VFF["form.validateFields"]
  VFF --> BE["buildEmptyProcessYaml(meta)"]
  BE --> YD["yamlDump(emptyProcess)"]
  CM --> API["bundledApi.postProcess(meta)"]
  API --> POST["POST /api/v1/processes"]
  POST --> H["backend create_process"]
  H --> VN["validate_process_name + validate_name_regex"]
  H --> DUP["db.get_process_template_by_name 唯一性"]
  H --> PAR["serde_yaml::from_str ProcessDefinition"]
  H --> AW["atomic_write target_path"]
  AW --> IMP["import_user_process_templates 刷新入库"]
  CM --> OC["onCreated(meta.guid) → 父组件 pushUrl"]
```

## 数据结构图

```mermaid
classDiagram
  class ProcessMetaInput {
    +name: string
    +guid: string
    +display_name: string
    +description?: string
    +category?: string
    +complexity?: string
    +version?: string
  }
  class CreateProcessRequest {
    +name: String
    +display_name: Option~String~
    +category: Option~String~
    +complexity: Option~String~
    +version: Option~String~
    +definition: String
  }
  class ProcessDefinition {
    +process: ProcessMeta
    +phases: Vec~PhaseDefinition~
  }
  class ProcessTemplate {
    +id: i64
    +guid: String
    +name: String
    +display_name: String
    +category: String
    +complexity: String
    +version: String
    +source_path: Option~String~
    +is_system: bool
  }
  ProcessMetaInput --> CreateProcessRequest: 前端构造 body
  CreateProcessRequest --> ProcessDefinition: serde_yaml 解析
  ProcessDefinition --> ProcessTemplate: 导入入库
```

## 数据变更图

```mermaid
stateDiagram-v2
  [*] --> 表单未填
  表单未填 --> 表单校验中: 用户输入 name
  表单校验中 --> 表单校验中: debounce 比对 existingNames
  表单校验中 --> 提交中: 确认提交
  提交中 --> 已创建: 后端 201 + 写盘 + 入库
  提交中 --> 表单校验中: 409 同名 / 400 YAML 错误
  已创建 --> 编辑器态: pushUrl processes?processMode=edit&guid
  编辑器态 --> [*]
```

## 开发指导

- **前端入口**：`frontend/src/components/process/CreateProcessMetaModal.tsx` 的 `CreateProcessMetaModal` 组件；`buildEmptyProcessYaml` 在 `frontend/src/components/process/buildEmptyProcessYaml.ts`
- **后端入口**：`backend/src/handlers/process.rs` 的 `create_process` handler，路由 `POST /api/v1/processes`
- **注意**：`name` 只能含 `[a-zA-Z0-9_-]`，前端 debounce 校验只做唯一性兜底，最终防线在后端 409；guid 由前端 `crypto.randomUUID()` 生成并写入 YAML，后端不再二次生成
- **扩展**：新增元信息字段时，在 `ProcessMetaInput` 加字段、`buildEmptyProcessYaml` 决是否写入、`CreateProcessRequest` 加对应 Option 字段、`buildEmptyProcessYaml` 的 processObj 决输出键
