# 前端规范 04：Ant Design 组件规范

> 定义 Ant Design 5.x 的使用规范。

---

## 1. 组件选择

优先使用 Ant Design 内置组件，避免自造轮子：

| 场景 | 推荐组件 |
|------|---------|
| 表单 | `Form` + `Form.Item` |
| 表格 | `Table`（非手写 table） |
| 弹窗 | `Modal` / `Drawer` |
| 下拉菜单 | `Dropdown` / `Select` |
| 通知 | `message` / `notification` |
| 空状态 | `Empty` |
| 加载 | `Spin` / `Skeleton` |
| 分页 | `Pagination` |
| 日期 | `DatePicker` / `TimePicker` |
| 图标 | `@ant-design/icons` |

---

## 2. 弹窗浮层问题修复

Ant Design 弹窗（`Dropdown`、`Modal`、`Select`、`DatePicker` 等）在某些上下文
（如桌面端配置菜单）可能点击弹不出来，原因是 `getPopupContainer` 未正确设置：

```tsx
// 在根组件中统一配置 getPopupContainer，将浮层挂载到 body 下，
// 避免被父容器的 overflow: hidden 裁剪导致无法弹出。
import { ConfigProvider } from 'antd';

<ConfigProvider
  getPopupContainer={() => document.body}
>
  <App />
</ConfigProvider>
```

---

## 3. Table 配置

```tsx
// Table 列配置使用 useMemo 缓存，避免每次渲染都重建导致子组件重渲染。
const columns = useMemo<ColumnsType<Todo>>(() => [
  {
    title: '标题',
    dataIndex: 'title',
    key: 'title',
  },
  {
    title: '状态',
    dataIndex: 'status',
    key: 'status',
    // 使用枚举映射，而非硬编码字符串
    render: (status: TodoStatus) => STATUS_MAP[status],
  },
], []);
```

---

## 4. 主题配置

使用 Ant Design 的 `ConfigProvider` 主题配置，而非全局 CSS 覆盖：

```tsx
// 在 ConfigProvider 的 theme 属性中定制 token，
// 优先级高于全局 CSS，且不与 Ant Design 内部样式冲突。
<ConfigProvider
  theme={{
    token: {
      colorPrimary: '#1677ff',
      borderRadius: 6,
    },
  }}
>
  <App />
</ConfigProvider>
```
