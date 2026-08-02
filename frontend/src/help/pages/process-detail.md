# 查看工艺详情

## 功能位置

工艺 → 工艺卡片 actions「详情」按钮 或 URL 携 guid 参数自动打开 → 弹出详情 Modal（流程图 / 实例环路 / YAML 源 三 Tab）

## 数据流图（前端 → 后端）

```mermaid
flowchart LR
  U[用户点详情] --> HSD["handleShowDetail(guid)"]
  HSD --"getProcess(guid)"--> API1["/api/bundled/processes/{guid}"]
  API1 --> H1["get_process_template handler"]
  H1 --"get_process_template_by_guid"--> DB[(process_templates 表)]
  H1 --"read_definition(source_path)"--> FS["磁盘读 YAML 正文"]
  HSD --> SET["setDetail(data)"]
  SET --> MOD["详情 Modal 渲染"]
  MOD --"切实例环路 Tab"--> FIL["fetchInstanceLoops(guid)"]
  FIL --"listProcessLoops(guid)"--> API2["/api/v1/processes/{guid}/loops"]
  API2 --> H2["list_process_loops handler"]
  H2 --"list_loops_by_process_template"--> DB2[(loops 表)]
  H2 --"count_loop_executions_by_loop_ids"--> DB3[(loop_executions 表)]
  MOD --"实例环路行更新"--> HUL["handleUpgradeLoop(guid, loopId)"]
  HUL --"upgradeProcessLoop(guid, loopId)"--> API3["/api/v1/processes/{guid}/loops/{loopId}/upgrade"]
```

## 调用关系链路图

```mermaid
flowchart TD
  HSD["handleShowDetail"] --> API["bundledApi.getProcess(guid)"]
  API --> GET["GET /api/bundled/processes/{guid}"]
  GET --> H["backend get_process_template"]
  H --> GT["db.get_process_template_by_guid"]
  H --> RD["read_definition(source_path, local_path)"]
  H --> RT["ApiResponse.ok(ProcessTemplateDetail)"]
  MOD["Modal Tabs onChange"] --> FIL["fetchInstanceLoops"]
  FIL --> API2["bundledApi.listProcessLoops"]
  API2 --> GET2["GET /api/v1/processes/{guid}/loops"]
  GET2 --> H2["backend list_process_loops"]
  H2 --> LL["db.list_loops_by_process_template"]
  H2 --> CE["db.count_loop_executions_by_loop_ids"]
  HUL["handleUpgradeLoop"] --> API3["bundledApi.upgradeProcessLoop"]
  API3 --> POST3["POST /api/v1/processes/{guid}/loops/{loopId}/upgrade"]
```

## 数据结构图

```mermaid
classDiagram
  class ProcessTemplateDetail {
    +id: i64
    +guid: String
    +name: String
    +display_name: String
    +description: String
    +category: String
    +complexity: String
    +version: String
    +source_path: Option~String~
    +is_system: bool
    +definition: String
  }
  class ProcessLoopItem {
    +id: i64
    +name: String
    +status: String
    +workspace_id: Option~i64~
    +process_template_version: Option~String~
    +created_at: Option~String~
    +execution_count: i64
  }
  class InstallProcessResponse {
    +loop_id: i64
    +loop_name: String
    +phase_count: usize
    +step_count: usize
  }
  class ProcessFlowGraph {
    +links: Link[]
    +nodeInputs: NodeInput[]
    +edgeInputs: EdgeInput[]
    +templateEdges: Edge[]
    +phaseGroups: PhaseGroup[]
  }
  ProcessTemplateDetail --> ProcessFlowGraph: adaptProcessDefinition
  ProcessTemplateDetail --> ProcessLoopItem: 实例环路 Tab
  ProcessLoopItem --> InstallProcessResponse: upgrade 返回
```

## 数据变更图

```mermaid
stateDiagram-v2
  [*] --> 加载中
  加载中 --> 流程图Tab: getProcess 成功
  流程图Tab --> 实例环路Tab: 用户切 Tab
  实例环路Tab --> 加载实例中: fetchInstanceLoops
  加载实例中 --> 实例环路Tab: list 返回
  实例环路Tab --> 升级中: 点更新
  升级中 --> 实例环路Tab: upgrade 成功 刷新
  实例环路Tab --> YAML源Tab: 用户切 Tab
  YAML源Tab --> [*]: 关闭 Modal
```

## 开发指导

- **前端入口**：`frontend/src/components/ProcessPage.tsx` 的 `handleShowDetail` / `fetchInstanceLoops` / `handleUpgradeLoop`；流程图组件 `frontend/src/components/process/ProcessFlowGraph.tsx`，适配器 `adaptProcessDefinition`
- **后端入口**：`backend/src/handlers/process.rs` 的 `get_process_template` / `list_process_loops` / `upgrade_process_loop` handler；升级 service 在 `backend/src/services/process/installer.rs` 的 `upgrade_process_template_loop`
- **注意**：工艺正文不在 DB，按 `source_path` 从磁盘实时读取；`upgrade_process_template_loop` 是不可逆操作——旧步骤 todo 软删但历史执行数据保留；URL 携 guid 时 useEffect 自动打开详情并用 cancelled flag 防竞态
- **扩展**：详情 Modal 增新 Tab 时，在 `Tabs.items` 加项、在 `onChange` 按需触发懒加载 API
