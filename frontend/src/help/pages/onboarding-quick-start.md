# 快速开始 5 步

## 功能位置

导航（概念首页） → section「快速开始」 → `QuickStartFlow` 横向流程图（4 步：安装工艺 / 创建任务 / 监控执行 / 验收产物）

注：044 触发器整体下线后，快速开始已从 5 步收敛为 4 步，文件名保留「5 步」为历史命名。

## 数据流图（前端 → 后端）

```mermaid
flowchart LR
  U[用户进入导航页] --> QSF["QuickStartFlow workspaceId"]
  QSF --"useConceptCounts(workspaceId)"--> UCC["hook 并行拉 6 API"]
  UCC --"bundledApi.getProcesses()"--> API1["/api/bundled/processes"]
  UCC --"dbLoops.listLoops(wsId)"--> API2["/api/v1/loops?workspace_id="]
  UCC --"db.getAllTodos(wsId)"--> API3["/api/v1/workspaces/{ws}/todos"]
  UCC --"bundledApi.listTasks(wsId)"--> API4["/api/v1/workspaces/{ws}/tasks"]
  UCC --"db.getExecutors()"--> API5["/api/v1/executors"]
  UCC --"db.getAllExperts()"--> API6["/api/v1/experts"]
  UCC --> QS["quickStart 完成状态 4 步"]
  QS --> SN["StepNode done 状态图标"]
  U --"点击节点"--> HG["handleGoto(navTarget)"]
  HG --"pushUrl(target, {})"--> NAV["跳转对应操作页"]
```

## 调用关系链路图

```mermaid
flowchart TD
  QSF["QuickStartFlow"] --> UCC["useConceptCounts(workspaceId)"]
  UCC --> PA["Promise.all 6 个 API 并行"]
  PA --> C1["processes 非空 → 步骤1 done"]
  PA --> C2["tasks 非空 → 步骤2 done"]
  PA --> C3["getExecutionRecords 非空 → 步骤3 done"]
  PA --> C4["暂用步骤3同源 → 步骤4 done"]
  QSF --> MAP["QUICK_START_STEPS.map(step)"]
  MAP --> SN["StepNode done/step/onGoto"]
  SN --> IC["statusIcon done=true 绿✓ / false 灰○ / null 灰○"]
  SN --> HG["onGoto → handleGoto"]
  HG --> PUSH["pushUrl(step.navTarget, {})"]
```

## 数据结构图

```mermaid
classDiagram
  class QuickStartStep {
    +index: number
    +title: string
    +navTarget: View
    +checkApi: 'processes'|'tasks'|'executions'|'artifacts'
  }
  class QUICK_START_STEPS {
    +step1: 安装工艺 → processes
    +step2: 创建任务 → tasks
    +step3: 监控执行 → memorial
    +step4: 验收产物 → loops
  }
  class QuickStartStatus {
    +1: boolean
    +2: boolean
    +3: boolean
    +4: boolean
  }
  class UseConceptCountsResult {
    +counts: ConceptCounts | null
    +quickStart: QuickStartStatus | null
    +loading: boolean
  }
  QuickStartStep --> QUICK_START_STEPS: as const 数组
  UseConceptCountsResult --> QuickStartStatus: quickStart 字段
  QuickStartStatus --> QuickStartStep: 按 index 判断
```

## 数据变更图

```mermaid
stateDiagram-v2
  [*] --> 加载中: loading=true quickStart=null
  加载中 --> 已渲染: Promise.all 返回 quickStart 就绪
  已渲染 --> 已渲染: 步骤完成 刷新后 done=true
  已渲染 --> 跳转态: 点击节点
  跳转态 --> [*]
  加载中 --> 已渲染: 部分失败 降级 null
```

## 开发指导

- **前端入口**：`frontend/src/components/onboarding/QuickStartFlow.tsx` 的 `QuickStartFlow` 组件；步骤定义在 `frontend/src/components/onboarding/concepts.tsx` 的 `QUICK_START_STEPS` 常量；完成状态在 `frontend/src/hooks/useConceptCounts.ts` 的 `useConceptCounts` hook
- **后端入口**：完成判断拉 6 个 REST API（processes/loops/todos/tasks/executors/experts），并行 `Promise.all` 每个 catch 为 null 永不 reject
- **注意**：步骤 4（验收产物）暂用步骤 3 同源判断（完整实现需扫审计 API，YAGNI 阶段先简化）；`done=null` 表示拉取失败/未拉取，渲染灰色问号兜底；横向流程图节点固定宽度 120px 避免间距不均；跳转走 `pushUrl` 触发 ntd-nav-change 事件全站同步
- **扩展**：恢复/新增步骤时，改 `QUICK_START_STEPS` 数组 + `QuickStartStep` 的 `checkApi` 联合类型；`useConceptCounts` 增对应完成判断 API 调用
