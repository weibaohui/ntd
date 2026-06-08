# 标签管理

> **位置**：设置 →标签管理
> **前端**：`frontend/src/components/settings/TagsPanel.tsx`
> **后端**：`backend/src/handlers/tag.rs`

简单的标签 CRUD。标签用于：
- 在 Todo列表里按标签筛选
- 在 TodoDrawer给 Todo打多个标签
-在仪表盘按标签统计

---

## 1.数据模型

|字段 |含义 |
|------|------|
| `id` |内部 ID |
| `name` |标签名（必填，全局唯一） |
| `color` |颜色（hex `#RRGGBB`，UI 里选） |
| `created_at` |创建时间 |

---

## 2.操作

>当前 UI（`frontend/src/components/settings/TagsPanel.tsx`）只暴露「**新增**」和「**删除**」两个操作，**没有**「编辑」按钮。

|操作 |入口 |
|------|------|
| 新增 |右上「+ 新增标签」 →填 name +选 color →创建 |
|删除 |列表点垃圾桶图标 → Popconfirm二次确认 |

### 2.1新增

1. 点「+」
2.填 name +选 color（色板）
3.保存

### 2.2删除

-删标签会**自动从所有关联 Todo上摘掉**
-不可恢复（除非你做了 Todo备份）

---

## 3.排序

-列表按 name字典序展示
- 后端 `tags::Column::Name.asc()`

---

## 4.颜色建议

-业务类（review/test）：蓝/绿
-风险类（bug/security）：红/橙
-阶段类（todo/doing/done）：浅色调

具体可以参考你的团队规范。

---

## 5.故障排查

### 5.1删除失败「有关联 Todo」

- 这个提示是旧版行为，新版会**直接级联删除关联**
- 如果你不想这样：先去 Todo列表把标签从 Todo上摘掉，再删标签

### 5.2标签名重复

- 后端有唯一约束
-改成不同名字

### 5.3颜色显示不对

- 部分老浏览器对 hex大小写敏感
- 用 `#xxxxxx`全小写

---

## 6.相关 API

| Method | Path |
|--------|------|
| GET | `/api/tags` |
| POST | `/api/tags` |
| DELETE | `/api/tags/{id}` |

> ⚠️ **目前不支持编辑标签名/颜色**。如果想改名字或颜色，请**先删再建**（注意：删除会摘掉所有关联）。后续如需编辑，应在 `frontend/src/components/settings/TagsPanel.tsx` 与 `backend/src/handlers/tag.rs` 增加 `PUT /api/tags/{id}`。
