# 工艺

> 页面级总览。本页各功能点的 4 图 + 开发指导在子文档中维护。

## 页面简介

工艺页是「工艺模板库」的浏览与管理入口。它把系统工艺（bundled 同步过来的内置模板）与用户工艺（`~/.ntd/processes/` 下自定义 YAML）统一成卡片网格，提供「我的/模板」视图切换、按描述推荐工艺、查看流程图详情、安装为环路、复制为我的工艺、进入可视化/YAML 编辑器等能力。

页面顶层由 `ProcessPage` 按 `processMode` 分流：列表态走 `ProcessListView`，编辑态走 `ProcessEditor`，新建态走 `ProcessEditorPlaceholder`。工艺正文不在数据库，按 `source_path` 从磁盘文件实时读取（DB 仅存元信息），保证磁盘是唯一真源。

## 页面级数据流总图

```mermaid
flowchart LR
  U[用户进入工艺页] --> PL["ProcessPage 按 processMode 分流"]
  PL --> LV["ProcessListView 列表态"]
  PL --> ED["ProcessEditor 编辑态"]
  LV --"getProcesses(scope)"--> API1["/api/bundled/processes?is_system="]
  LV --"getProcess(guid)"--> API2["/api/bundled/processes/{guid}"]
  LV --"installProcess(guid, wsId)"--> API3["/api/bundled/processes/{guid}/install"]
  LV --"copyProcessToUser(guid)"--> API4["/api/v1/processes/{guid}/copy-to-user"]
  LV --"recommendProcesses(desc)"--> API5["/api/v1/processes/recommend"]
  LV --"listProcessLoops(guid)"--> API6["/api/v1/processes/{guid}/loops"]
  LV --"upgradeProcessLoop(guid, loopId)"--> API7["/api/v1/processes/{guid}/loops/{loopId}/upgrade"]
  ED --"getProcess(guid)"--> API2
  ED --"putProcess(guid, yaml)"--> API8["/api/v1/processes/{guid}"]
  ED --"deleteProcess(guid)"--> API9["/api/v1/processes/{guid}"]
  API1 --> H1["list_process_templates handler"]
  API2 --> H2["get_process_template handler"]
  API3 --> H3["install_process handler"]
  API4 --> H4["copy_process_to_user handler"]
  API5 --> H5["recommend_process handler"]
  API6 --> H6["list_process_loops handler"]
  API7 --> H7["upgrade_process_loop handler"]
  API8 --> H8["update_process handler"]
  API9 --> H9["delete_process handler"]
  H1 --> DB1[(process_templates 表)]
  H2 --> DB1
  H2 --> FS1["磁盘 ~/.ntd 或 bundled 读 YAML"]
  H3 --> SVC["install_process_template service"]
  SVC --> DB2[(loops / loop_phases / loop_steps / todos 表)]
  H4 --> FS1
  H8 --> FS1
```

## 功能点索引

- [创建工艺](process-create)
- [我的/模板视图切换](process-scope-switch)
- [安装到工作空间](process-install)
- [查看工艺详情](process-detail)
- [编辑工艺（进编辑器）](process-edit)
- [复制为我的工艺](process-copy)
