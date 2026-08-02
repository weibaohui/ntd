# 安装到工作空间

## 功能位置

工艺 → 工艺卡片 actions「安装」按钮 或 详情 Modal 底部「安装到当前工作空间」按钮 → 弹确认 Modal → 确认 → 跳环路详情

## 数据流图（前端 → 后端）

```mermaid
flowchart LR
  U[用户点安装] --> HI["handleInstall(guid, displayName)"]
  HI --> SIM["setInstallModal({ guid, displayName })"]
  SIM --> MOD["确认 Modal 显示"]
  MOD --"确认 onOk"--> DI["doInstall()"]
  DI --"installProcess(guid, workspaceId)"--> API["POST /api/bundled/processes/{guid}/install"]
  API --> H["install_process handler"]
  H --"get_process_template_by_guid"--> DB1[(process_templates 表)]
  H --"get_project_directory_by_id"--> DB2[(project_directories 表)]
  H --"read_definition(source_path)"--> FS["磁盘读 YAML 正文"]
  H --> SVC["install_process_template service"]
  SVC --"create loop / phases / steps / todos"--> DB3[(loops/loop_phases/loop_steps/todos 表)]
  DI --"onOpenLoop(result.loop_id)"--> NAV["跳环路详情"]
```

## 调用关系链路图

```mermaid
flowchart TD
  HI["handleInstall"] --> SIM["setInstallModal"]
  SIM --> DI["doInstall"]
  DI --> WS["workspaceId 为空 → message.warning"]
  DI --> API["bundledApi.installProcess(guid, wsId)"]
  API --> POST["POST /api/bundled/processes/{guid}/install body={workspace_id}"]
  POST --> H["backend install_process"]
  H --> GT["db.get_process_template_by_guid"]
  H --> RD["read_definition 磁盘读 YAML"]
  H --> SVC["install_process_template(db, template, definition, ws_id, ws_path)"]
  SVC --> CL["create_loop loop_model"]
  SVC --> CPS["create_phases_and_steps"]
  CPS --> CPH["create_loop_phase 阶段"]
  CPS --> CTS["create_loop_step_for_link 环节+todo"]
  SVC --> RGT["resolve_goto_targets goto 解析"]
  DI --> NAV["onOpenLoop(result.loop_id)"]
```

## 数据结构图

```mermaid
classDiagram
  class InstallProcessRequest {
    +workspace_id: i64
  }
  class InstallProcessResponse {
    +loop_id: i64
    +loop_name: String
    +phase_count: usize
    +step_count: usize
  }
  class InstallResult {
    +loop_id: i64
    +loop_name: String
    +phase_count: usize
    +step_count: usize
  }
  class PhaseDefinition {
    +name: String
    +spec: String
    +links: Vec~LinkDefinition~
  }
  class LinkDefinition {
    +id: String
    +name: String
    +skills: Vec~String~
  }
  InstallProcessRequest --> InstallProcessResponse: handler 返回
  InstallProcessResponse --> InstallResult: service 产出
  InstallResult --> PhaseDefinition: 聚合阶段数
  PhaseDefinition --> LinkDefinition: 聚合环节数
```

## 数据变更图

```mermaid
stateDiagram-v2
  [*] --> 待确认
  待确认 --> 安装中: 确认 Modal onOk
  安装中 --> 已安装: 201 + loop_id
  已安装 --> 环路详情态: onOpenLoop(loop_id)
  安装中 --> 待确认: 失败 message.error
  环路详情态 --> [*]
```

## 开发指导

- **前端入口**：`frontend/src/components/ProcessPage.tsx` 的 `handleInstall` 与 `doInstall`；`bundledApi.installProcess` 在 `frontend/src/api/bundled.ts`
- **后端入口**：`backend/src/handlers/process.rs` 的 `install_process` handler；service 在 `backend/src/services/process/installer.rs` 的 `install_process_template`
- **注意**：`workspaceId` 为空时前端弹 warning 不发请求；安装即实例化——创建 loop/loop_phases/loop_steps/todos 一整链；goto 目标在二遍解析（先建 step 收集 id 映射，再 resolve_goto_targets）
- **扩展**：新增环节级字段时，改 `LinkDefinition`、`create_loop_step_for_link` 写入 loop_steps 列、`create_todo_for_link` 同步事项列
