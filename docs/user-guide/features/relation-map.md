# 关系图

> **位置**：Todo 列表 → 顶部「关系图」切换
> **前端**：`frontend/src/components/relation-map/RelationMap.tsx`

把 Todo 之间的**关联**画成图谱。适合看一个大任务被拆成几个子任务的结构。

## 1. 节点

- 每个节点 = 一个 Todo
- 颜色按状态：pending 灰、running 蓝、completed 绿、failed 红
- 节点大小 = 子任务数

## 2. 边

- 节点之间的连线 = `todo_hooks` 或手动关联
- 单向（parent → child）或双向（depends-on）
- 鼠标 hover 看关联类型

## 3. 交互

- 拖拽节点改变位置
- 滚轮缩放
- 双击节点 → 跳到 Todo 详情
- 右上角「**新建关联**」→ 选两个 Todo + 关联类型

## 4. 布局

- 力导向布局（d3-force）
- 自动把无关联的节点推到边上
- 强关联的聚在一起

## 5. 性能

- 上限 ~200 节点
- 多了会卡（前端 React 渲染压力）
- 解决：先用「时间筛选」缩小范围

## 6. 与 Hook 系统的关系

Todo 的前后置 hook 也会显示在关系图里。详细看 [Hook 系统设计](../../../hook-system-design.md)。

## 7. 故障排查

### 7.1 节点全聚一起

- 关联太多太密
- 拖拽一下手动散开

### 7.2 边不显示

- 关联没存
- 看 Todo 详情 → Hook 编辑器是否有内容

### 7.3 卡顿

- Todo 太多，加筛选
- 关掉浏览器其他 tab
