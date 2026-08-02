# 概念关系图

## 功能位置

导航（概念首页） → section「概念关系图」 → `ConceptRelationGraph` SVG 图（主链 4 节点 + 支线 6 节点 + hover 高亮 + 点击弹 Drawer）

## 数据流图（前端 → 后端）

```mermaid
flowchart LR
  U[用户进入导航页] --> CNP["ConceptNavPage"]
  CNP --> CRG["ConceptRelationGraph"]
  CRG --> GN["GRAPH_NODES 常量 10 节点"]
  CRG --> GE["GRAPH_EDGES 常量 11 边"]
  CRG --> SVG["纯 SVG 渲染 圆+线+箭头"]
  U --"hover 节点"--> HL["高亮关联节点 highlights 列表"]
  U --"点击节点"--> DR["Drawer 弹详情"]
  DR --"conceptId 命中 CONCEPTS"--> CDS["ConceptNode fields 表"]
  DR --"支线节点 navTarget"--> PUSH["pushUrl(navTarget, navMode)"]
```

## 调用关系链路图

```mermaid
flowchart TD
  CRG["ConceptRelationGraph"] --> GN["GRAPH_NODES: 主链 process/loop/todo/execution + 支线 task/executor/expert/skill/model/blackboard/kanban"]
  CRG --> GE["GRAPH_EDGES: 主航线 3 边 + 支线 8 边"]
  CRG --> EP["edgePath fromR/toR 边缘偏移"]
  CRG --> NR["nodeRadius isMain ? 48 : 36"]
  CRG --> E["Edge 组件 主航线带箭头加粗"]
  CRG --> N["Node 组件 hover/onClick"]
  N --> SS["useState selectedNode"]
  N --> DR["Drawer open"]
  DR --> FIND["CONCEPTS.find(id === conceptId)"]
  FIND --> CDS["Descriptions fields 表"]
  DR --> NAV["支线节点 navTarget → pushUrl"]
```

## 数据结构图

```mermaid
classDiagram
  class GraphNode {
    +id: string
    +label: string
    +x: number
    +y: number
    +highlights: string[]
    +conceptId?: ConceptId
    +isMain?: boolean
    +drawerDesc?: string
    +navTarget?: View
    +navMode?: BoardMode
  }
  class GraphEdge {
    +from: string
    +to: string
    +label: string
    +isMain?: boolean
  }
  class ConceptNode {
    +id: ConceptId
    +label: string
    +oneLiner: string
    +fields: Field[]
    +navTarget: View
    +yamlExample: string
  }
  GraphNode --> ConceptNode: conceptId 关联
  GraphEdge --> GraphNode: from/to 引用
```

## 数据变更图

```mermaid
stateDiagram-v2
  [*] --> 静态图
  静态图 --> hover高亮: 鼠标入节点
  hover高亮 --> 静态图: 鼠标出
  静态图 --> Drawer开: 点击节点
  Drawer开 --> 静态图: 关闭 Drawer
  Drawer开 --> 跳转态: 支线节点点去XX页
  跳转态 --> [*]
```

## 开发指导

- **前端入口**：`frontend/src/components/onboarding/ConceptRelationGraph.tsx` 的 `ConceptRelationGraph` 组件；节点/边数据在 `frontend/src/components/onboarding/concepts.tsx` 的 `GRAPH_NODES` / `GRAPH_EDGES` 常量
- **后端入口**：纯展示组件，不直接调后端；概念数量徽标由 `ConceptCardGrid` 调 `useConceptCounts` 拉取
- **注意**：不引 reactflow 重依赖（节点固定 10 个手布局）；尊重 `prefers-reduced-motion` 动画降级为静态高亮；支线节点 Drawer 支定制 `drawerDesc` + 跳转按钮（黑板/看板）；跳转必须走 `pushUrl` 不能用 `location.hash` 蜂跳（否则不触发 ntd-nav-change 事件全站同步）
- **扩展**：增支线节点时，在 `GRAPH_NODES` 加项（x/y/highlights/conceptId 或 drawerDesc/navTarget）、`GRAPH_EDGES` 加连线；新增有独立页的支线节点用 `navTarget` + `navMode` 三字段向后兼容
