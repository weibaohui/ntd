# 复制为我的工艺

## 功能位置

工艺 → 模板视图卡片 actions「复制」按钮 或 详情 YAML Tab（系统工艺）警示条旁「复制为我的工艺后编辑」按钮

## 数据流图（前端 → 后端）

```mermaid
flowchart LR
  U[用户点复制] --> HCT["handleCopyToUser(guid)"]
  HCT --"copyProcessToUser(guid)"--> API["POST /api/v1/processes/{guid}/copy-to-user"]
  API --> H["copy_process_to_user handler"]
  H --"get_process_template_by_guid"--> DB1[(process_templates 表)]
  H --"read_definition(source_path)"--> FS["磁盘读源 YAML"]
  H --"new_guid / replace_or_insert_guid"--> YN["副本换新 guid"]
  H --"compute_copy_target_path"--> TP["计算目标路径 冲突加后缀"]
  H --"atomic_write"--> FS2["写盘 ~/.ntd/processes/<rel>"]
  H --"import_user_process_templates"--> DB2["刷新入库 is_system=false"]
  HCT --"成功 message.success"--> HSC["handleScopeChange('mine')"]
  HSC --> LV["切到我的视图 立即看到副本"]
```

## 调用关系链路图

```mermaid
flowchart TD
  HCT["handleCopyToUser"] --> CP["setCopying(guid)"]
  HCT --> API["bundledApi.copyProcessToUser(guid)"]
  API --> POST["POST /api/v1/processes/{guid}/copy-to-user"]
  POST --> H["backend copy_process_to_user"]
  H --> GT["db.get_process_template_by_guid"]
  H --> RD["read_definition 源 YAML"]
  H --> NG["new_guid UUID v4"]
  H --> RG["replace_or_insert_guid 新 guid 单行替换"]
  H --> CP2["compute_copy_target_path 冲突加 -1/-2"]
  H --> AW["atomic_write 写副本"]
  H --> IMP["import_user_process_templates 重扫入库"]
  H --> RT["返回 user_source_path / guid / name"]
  HCT --> SU["message.success 已复制为我的工艺"]
  HCT --> HSC["handleScopeChange('mine') 切视图"]
```

## 数据结构图

```mermaid
classDiagram
  class CopyProcessResponse {
    +user_source_path: String
    +guid: String
    +name: String
  }
  class ProcessTemplate {
    +guid: String
    +name: String
    +source_path: Option~String~
    +is_system: bool
  }
  class ProcessTemplateListItem {
    +guid: String
    +name: String
    +is_system: bool
  }
  CopyProcessResponse --> ProcessTemplate: 副本换新 guid 同名共存
  ProcessTemplate --> ProcessTemplateListItem: 入库为 is_system=false
```

## 数据变更图

```mermaid
stateDiagram-v2
  [*] --> 复制中
  复制中 --> 已复制: 200 + 副本写盘 + 入库
  已复制 --> 我的视图态: handleScopeChange('mine')
  我的视图态 --> [*]
  复制中 --> [*]: 失败 message.error
```

## 开发指导

- **前端入口**：`frontend/src/components/ProcessPage.tsx` 的 `handleCopyToUser`；编辑器内也有同逻辑在 `frontend/src/components/process/ProcessEditor.tsx` 的 `handleCopyToUser`（副本跳副本编辑器）
- **后端入口**：`backend/src/handlers/process.rs` 的 `copy_process_to_user` handler；guid 生成在 `backend/src/services/process/guid.rs` 的 `new_guid` / `replace_or_insert_guid`
- **注意**：副本与源同名共存（guid 不同），原模板不消失；文件名冲突时自动加 `-1`/`-2` 后缀；复制成功后自动切「我的」视图，让用户立即看到新卡片并可点编辑
- **扩展**：副本需附源标记时，改 `replace_or_insert_guid` 在 YAML 注 `derived_from` 字元，`import_user_process_templates` 入库时读该元写 `derived_from_guid` 列
