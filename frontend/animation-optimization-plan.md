# ntd 前端动画与 UI 质感优化方案

> 本方案基于 `apple-design`、`emil-design-eng`、`find-animation-opportunities`、`improve-animations` 四大 skill 的分析结果编写。
> 执行者必须完整阅读本文档后再动手修改代码。

---

## 第一部分：执行守则（必读）

### 规则 1：禁止破坏现有功能
- 所有改动必须保持现有交互逻辑不变，只改视觉/动画层。
- 修改后必须运行 `cd frontend && npx tsc --noEmit` 确认零 TypeScript 错误。

### 规则 2：缓动曲线标准（来自 emil-design-eng）
- **永远不要使用 `ease-in`** 作为 UI 动画的缓动函数，它会让界面感觉 sluggish。
- **永远不要使用 `transition: all`**，必须指定具体属性。
- **永远不要从 `scale(0)` 开始动画**，从 `scale(0.95)` + `opacity: 0` 开始。
- **动画时长预算**：UI 元素必须 ≤300ms；按钮反馈 100-160ms；下拉菜单 150-250ms；模态框/抽屉 200-500ms。
- **自定义缓动曲线**：
  ```css
  --ease-out: cubic-bezier(0.23, 1, 0.32, 1);        /* UI 交互主力 */
  --ease-in-out: cubic-bezier(0.77, 0, 0.175, 1);    /* 对称运动 */
  --ease-drawer: cubic-bezier(0.32, 0.72, 0, 1);     /* 抽屉/面板 */
  ```

### 规则 3：频率决策矩阵（来自 find-animation-opportunities）
| 频率 | 决策 |
|------|------|
| 100+ 次/天（键盘快捷键、命令面板） | **绝不添加动画** |
| 数十次/天（hover、列表导航） | 只允许几乎不可察觉的微动画 |
| 偶尔（模态框、抽屉、toast） | 标准动画 |
| 罕见/首次（空状态、成功反馈） | 可以添加愉悦感动画 |

### 规则 4：可访问性（来自 apple-design）
- 必须添加 `@media (prefers-reduced-motion: reduce)` 支持。
- 减少动画不等于零动画：保留 opacity/color 过渡，移除 transform 运动。
- hover 效果必须用 `@media (hover: hover) and (pointer: fine)` 限制在指针设备上。

### 规则 5：弹簧动画（来自 apple-design）
- **可打断性**是最高原则：任何手势驱动的动画必须能被随时打断并反向。
- 弹簧参数：默认用 `{ damping: 1.0, response: 0.3-0.4 }`（临界阻尼，无过冲）。
- 只有**手势带有动量**时才允许轻微过冲（`damping ~0.8`）。
- 速度交接：手势结束时，动画必须继承手指的释放速度，不能从零开始。

### 规则 6：性能铁律
- **只动画 `transform` 和 `opacity`**，这两个属性在 GPU 上合成，不触发布局/绘制。
- CSS 动画在主线程之外运行，比 JS 动画（`requestAnimationFrame`）在负载下更流畅。
- 禁止动画 `width`、`height`、`padding`、`margin`、`top`、`left`。

### 规则 7：按压反馈（来自 emil-design-eng）
- **所有可按压元素必须有 `:active` 状态**：`transform: scale(0.97)` + `transition: transform 120ms var(--ease-out)`。
- 按压反馈是用户对界面响应感知的基础，不可省略。

### 规则 8：空间一致性（来自 apple-design）
- 弹出层（Popover、菜单）必须从**触发点**缩放进入，不能从中心。
- 进入和退出必须沿**同一路径**。
- 模态框除外——模态框从视口中心缩放是正确的。

---

## 第二部分：改动清单（按优先级排序）

| 编号 | 优先级 | 改动项 | 文件 | 预估工作量 |
|------|--------|--------|------|-----------|
| A1 | 🔴 HIGH | 升级全局缓动曲线变量 | `App.css` | 10分钟 |
| A2 | 🔴 HIGH | 全局按钮 `:active` 按下反馈 | `App.css` | 15分钟 |
| A3 | 🔴 HIGH | 修复 `transition: all` 性能问题 | `App.css` 多处 | 20分钟 |
| B1 | 🟡 MEDIUM | 左侧导航栏玻璃态 + 按压反馈 | `App.css` | 20分钟 |
| B2 | 🟡 MEDIUM | 配置菜单 Popover 空间一致性 | `App.css` | 15分钟 |
| B3 | 🟡 MEDIUM | 事项卡片按压反馈 + 入口动画 | `App.css` | 15分钟 |
| B4 | 🟡 MEDIUM | 全局 `prefers-reduced-motion` 完善 | `App.css` | 15分钟 |
| C1 | 🟢 LOW | FAB 展开/收缩 stagger 动画 | `App.css` + `FloatingActionButton.tsx` | 25分钟 |
| C2 | 🟢 LOW | Dashboard Tab 切换内容过渡 | `App.css` + `Dashboard.tsx` | 20分钟 |
| C3 | 🟢 LOW | 执行面板展开/折叠平滑过渡 | `App.css` | 10分钟 |
| C4 | 🟢 LOW | 字体排版微调（字间距） | `App.css` | 10分钟 |
| C5 | 🟢 LOW | Modal/Drawer 动画统一优化 | `App.css` | 15分钟 |

---

## 第三部分：详细改动说明

### A1. 升级全局缓动曲线变量

**文件**：`frontend/src/App.css`
**当前状态**（约第 71-74 行）：
```css
/* Transitions */
--transition-fast: 150ms ease;
--transition-base: 200ms ease;
--transition-slow: 300ms ease;
```

**修改后**：
```css
/* Custom easing curves — stronger than browser defaults for intentional, snappy feel */
--ease-out: cubic-bezier(0.23, 1, 0.32, 1);        /* UI interactions: fast start, gentle settle */
--ease-in-out: cubic-bezier(0.77, 0, 0.175, 1);    /* Symmetric motion: both ends smooth */
--ease-drawer: cubic-bezier(0.32, 0.72, 0, 1);     /* Drawer/sheet: iOS-like smoothness */
--ease-linear: linear;

/* Transitions — shorter durations + custom easing for perceived responsiveness */
--transition-fast: 120ms var(--ease-out);
--transition-base: 200ms var(--ease-out);
--transition-slow: 300ms var(--ease-out);
```

**为什么**：
- `ease` 是浏览器默认弱缓动，缺乏力度，让界面感觉 sluggish。
- `cubic-bezier(0.23, 1, 0.32, 1)` 是 Emil Kowalski 推荐的强 ease-out：开始快、结束柔，用户立刻看到响应。
- 时长从 150ms 降到 120ms（fast），感知更快。

**验证**：
- 修改后 hover 任意按钮，感受是否更 snappy。
- 使用浏览器 DevTools Animations 面板，确认缓动曲线显示为自定义 cubic-bezier。

---

### A2. 全局按钮 `:active` 按下反馈

**文件**：`frontend/src/App.css`
**位置**：在 `:focus-visible` 规则之后（约第 264 行后）插入新规则块。

**当前状态**：全局没有统一的 `:active` 按下反馈。只有 FAB 按钮有 `.fab-collapse-btn:active` 和 `.fab-item-btn:active`。

**修改内容——添加全局按压反馈**：
```css
/* ============================================================
   Global Press Feedback — 所有可按压元素的即时响应
   原则：按压反馈必须在 pointer-down 时立即生效，不能等到 click。
   为什么：用户在按下瞬间就需要确认界面听到了自己。
   ============================================================ */

/* Ant Design 按钮按压反馈 */
.ant-btn:not(.ant-btn-disabled):active,
.ant-btn:not([disabled]):active {
  transform: scale(0.97);
  transition: transform 120ms var(--ease-out);
}

/* 主按钮（primary）可以压得更深一点，因为颜色更重 */
.ant-btn-primary:not(.ant-btn-disabled):active,
.ant-btn-primary:not([disabled]):active {
  transform: scale(0.95);
}

/* 危险按钮 */
.ant-btn-dangerous:not(.ant-btn-disabled):active,
.ant-btn-dangerous:not([disabled]):active {
  transform: scale(0.95);
}

/* 纯 button 元素（非 antd） */
button:not(:disabled):active {
  transform: scale(0.97);
  transition: transform 120ms var(--ease-out);
}

/* 链接/文本按钮不需要缩放，否则文字会模糊；改为背景色变化 */
.ant-btn-link:active,
.ant-btn-text:active {
  transform: none;
  opacity: 0.7;
  transition: opacity 120ms var(--ease-out);
}
```

**为什么**：
- 来自 Emil Design Engineering："Buttons must feel responsive to press".
- 来自 Apple Design："Respond on pointer-down, not on release."
- `scale(0.97)` 是微妙的——用户不会注意到它，但会感受到按钮"听话"。
- 时长 120ms 属于"即时反馈"范围，不延迟操作。
- `ant-btn-link` 和 `ant-btn-text` 不用 scale，因为文字缩放会导致模糊，改用 opacity。

**注意**：
- 不要给已有 `:active` 样式的元素重复添加（如 `.fab-collapse-btn:active`、`.fab-item-btn:active` 已存在且用 `scale(0.95)`，更激进，保留即可）。
- `:not(:disabled)` 防止禁用按钮也产生按压效果。

**验证**：
- 在任意页面点击按钮，肉眼应看到轻微缩小。
- 在 DevTools 中选中按钮，手动添加 `:active` 伪类，确认 `transform: scale(0.97)` 已应用。

---

### A3. 修复 `transition: all` 性能问题

**文件**：`frontend/src/App.css`
**当前问题位置**：以下 12 处使用了 `transition: all`：

| 行号 | 选择器 | 当前代码 |
|------|--------|----------|
| 333 | `.header-nav-btn, .header-overflow-btn` | `transition: all var(--transition-fast);` |
| 355 | `.header-primary-action` | `transition: all var(--transition-base, 0.2s ease);` |
| 407 | `.tag-chip` | `transition: all var(--transition-fast);` |
| 1147 | `.fab-collapse-btn` | `transition: all 0.15s ease;` |
| 1168 | `.fab-item-btn` | `transition: all 0.15s ease;` |
| 1547 | `.history-item-compact` | `transition: all var(--transition-fast);` |
| 1576 | `.tag-check-card` | `transition: all var(--transition-fast);` |
| 1737 | `.execution-tab` | `transition: all 0.2s ease;` |
| 1816 | `.panel-toggle-btn` | `transition: all 0.2s ease;` |
| 2457 | `.settings-tabs.ant-tabs-card > .ant-tabs-nav .ant-tabs-tab` | `transition: all var(--transition-fast);` |
| 3381 | `.todo-item` | `transition: all 0.15s;` |
| 3811 | `.kanban-card` | `transition: all 0.15s ease;` |

**逐处修改**：

1. **第 333 行** `.header-nav-btn, .header-overflow-btn`：
   ```css
   /* 原：transition: all var(--transition-fast); */
   transition: background var(--transition-fast), color var(--transition-fast), transform var(--transition-fast);
   ```

2. **第 355 行** `.header-primary-action`：
   ```css
   /* 原：transition: all var(--transition-base, 0.2s ease); */
   transition: background var(--transition-base), color var(--transition-base), border-color var(--transition-base), transform var(--transition-base), box-shadow var(--transition-base);
   ```

3. **第 407 行** `.tag-chip`：
   ```css
   /* 原：transition: all var(--transition-fast); */
   transition: background var(--transition-fast), color var(--transition-fast), border-color var(--transition-fast), box-shadow var(--transition-fast);
   ```

4. **第 1147 行** `.fab-collapse-btn`：
   ```css
   /* 原：transition: all 0.15s ease; */
   transition: color 120ms var(--ease-out), transform 120ms var(--ease-out), background 120ms var(--ease-out);
   ```

5. **第 1168 行** `.fab-item-btn`：
   ```css
   /* 原：transition: all 0.15s ease; */
   transition: transform 120ms var(--ease-out), box-shadow 120ms var(--ease-out), background 120ms var(--ease-out);
   ```

6. **第 1547 行** `.history-item-compact`：
   ```css
   /* 原：transition: all var(--transition-fast); */
   transition: border-color var(--transition-fast), background var(--transition-fast), box-shadow var(--transition-fast);
   ```

7. **第 1576 行** `.tag-check-card`：
   ```css
   /* 原：transition: all var(--transition-fast); */
   transition: border-color var(--transition-fast), background var(--transition-fast), box-shadow var(--transition-fast);
   ```

8. **第 1737 行** `.execution-tab`：
   ```css
   /* 原：transition: all 0.2s ease; */
   transition: background 200ms var(--ease-out), color 200ms var(--ease-out), border-bottom-color 200ms var(--ease-out);
   ```

9. **第 1816 行** `.panel-toggle-btn`：
   ```css
   /* 原：transition: all 0.2s ease; */
   transition: background 200ms var(--ease-out), color 200ms var(--ease-out);
   ```

10. **第 2457 行** `.settings-tabs.ant-tabs-card > .ant-tabs-nav .ant-tabs-tab`：
    ```css
    /* 原：transition: all var(--transition-fast); */
    transition: background var(--transition-fast), color var(--transition-fast), border-color var(--transition-fast), box-shadow var(--transition-fast);
    ```

11. **第 3381 行** `.todo-item`：
    ```css
    /* 原：transition: all 0.15s; */
    transition: border-color 120ms var(--ease-out), background 120ms var(--ease-out), box-shadow 120ms var(--ease-out);
    ```
    
    **注意**：`.todo-item.selected` 也有复杂的 `box-shadow` 多层阴影和渐变背景，需要确保 hover 和 selected 状态的过渡属性一致。

12. **第 3811 行** `.kanban-card`：
    ```css
    /* 原：transition: all 0.15s ease; */
    transition: border-color 120ms var(--ease-out), box-shadow 120ms var(--ease-out), transform 120ms var(--ease-out);
    ```

**为什么**：
- `transition: all` 会导致浏览器监听所有可动画属性的变化，包括那些我们并不关心的属性（如 `display`、`visibility` 变化时也会尝试过渡）。
- 只指定实际变化的属性，浏览器只需合成器处理这些属性，性能更好。
- 这是 Emil Design Engineering Review Checklist 中的硬性要求。

**验证**：
- 修改后，hover 每个对应元素，确认过渡效果与修改前视觉上无差异。
- 在 Chrome DevTools Performance 面板中录制一次 hover 交互，确认没有意外的布局/绘制触发。

---

### B1. 左侧导航栏玻璃态 + 按压反馈

**文件**：`frontend/src/App.css`

#### B1.1 导航栏玻璃态效果

**当前状态**（约第 4021-4029 行）：
```css
.ntd-left-rail {
  height: 100%;
  display: flex;
  flex-direction: column;
  align-items: center;
  padding: 10px 8px;
  background: color-mix(in srgb, var(--color-bg-elevated) 92%, var(--color-bg) 8%);
  border-right: 1px solid var(--color-border-light);
}
```

**修改后**：
```css
.ntd-left-rail {
  height: 100%;
  display: flex;
  flex-direction: column;
  align-items: center;
  padding: 10px 8px;
  /* 玻璃态：半透明背景 + 背景模糊，让下方内容隐约透出，营造层次 */
  background: rgba(255, 255, 255, 0.72);
  backdrop-filter: blur(20px) saturate(180%);
  -webkit-backdrop-filter: blur(20px) saturate(180%);
  /* 顶部亮边模拟光线照在玻璃上的效果 */
  border-right: 1px solid rgba(0, 0, 0, 0.06);
  /* 微妙的顶部高光 */
  box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.4);
}
```

**暗色主题适配**（在现有 `[data-theme="dark"] .ntd-left-rail` 或新增）：
```css
[data-theme="dark"] .ntd-left-rail {
  background: rgba(30, 30, 46, 0.72);
  backdrop-filter: blur(20px) saturate(180%);
  -webkit-backdrop-filter: blur(20px) saturate(180%);
  border-right: 1px solid rgba(255, 255, 255, 0.04);
  box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.06);
}
```

**为什么**：
- Apple Design 原则："Translucent materials as a floating functional layer that brings structure without stealing focus."
- 玻璃态让导航栏与内容区之间产生空间层次，不僵硬。
- `saturate(180%)` 让模糊背景的色彩更鲜艳，避免"浑浊"感。
- 顶部高光边（`inset 0 1px 0`）模拟物理光线，让玻璃更像真实材料。

**注意**：
- `backdrop-filter` 在 Safari 上需要 `-webkit-` 前缀。
- 如果浏览器不支持 `backdrop-filter`，会回退到纯色背景（`rgba`），仍然可用。

#### B1.2 导航按钮按压反馈

**当前状态**（约第 4185-4203 行）：`.ntd-left-rail-btn` 和 `.ntd-left-rail-expanded-btn` 都没有 `:active` 状态。

**修改——在 `.ntd-left-rail-btn.active` 之后添加**：
```css
.ntd-left-rail-btn:active {
  transform: scale(0.95);
  transition: transform 120ms var(--ease-out);
}
```

**在 `.ntd-left-rail-expanded-btn.active` 之后添加**：
```css
.ntd-left-rail-expanded-btn:active {
  transform: scale(0.98);
  transition: transform 120ms var(--ease-out);
}
```

**为什么**：
- 扩展按钮（带文字的）压得更少（0.98 vs 0.95），因为文字缩放会产生轻微模糊，需要更克制。
- 图标-only 按钮可以压得更明显（0.95），因为图标缩放不明显。

**验证**：
- 在桌面端点击左侧导航按钮，确认有轻微缩小感。
- 在 Safari 和 Chrome 中检查玻璃态效果是否一致。

---

### B2. 配置菜单 Popover 空间一致性

**文件**：`frontend/src/App.css`

**当前状态**（约第 4337-4398 行）：配置菜单 `.ntd-config-menu-popover` 没有 `transform-origin` 设置，会从中心缩放。

**修改——在 `.ntd-config-menu-popover .ant-popover-inner` 规则中添加**：
```css
.ntd-config-menu-popover .ant-popover-inner {
  padding: 0 !important;
  /* 空间一致性：菜单从触发按钮位置缩放进入，而非从中心 */
  transform-origin: bottom right;
  animation: config-menu-enter 200ms var(--ease-out) forwards;
}

@keyframes config-menu-enter {
  from {
    opacity: 0;
    transform: scale(0.95);
  }
  to {
    opacity: 1;
    transform: scale(1);
  }
}
```

**注意**：
- `transform-origin: bottom right` 假设配置按钮在左下角。如果配置按钮在抽屉模式下位置不同（左下角），抽屉模式下的 origin 也应是 `bottom left`。
- 对于 drawer 模式（移动端），需要单独设置：

```css
/* 移动端 drawer 模式：配置菜单从底部弹出 */
.ntd-left-rail-drawer-bottom .ntd-config-menu-popover .ant-popover-inner {
  transform-origin: bottom left;
}
```

**为什么**：
- Apple Design 原则："Anchor interactions to their source."
- 菜单从按钮位置生长出来，用户能感知菜单和按钮之间的空间关系。
- `scale(0.95)` 而非 `scale(0)`——Emil 原则："Nothing in the real world disappears and reappears completely."

**验证**：
- 点击配置按钮，观察菜单是否从按钮附近展开而非从中心放大。
- 使用 DevTools 选中菜单元素，检查 `transform-origin` 计算值。

---

### B3. 事项卡片按压反馈 + 入口动画

**文件**：`frontend/src/App.css`

#### B3.1 按压反馈

**当前状态**（约第 4563-4580 行）：`.todo-center-card` 已有 hover 效果，但没有 `:active` 按下反馈。

**修改——在 `.todo-center-card:hover` 之后添加**：
```css
.todo-center-card:active {
  /* 按下时轻微下沉，模拟物理按压 */
  transform: translateY(0) scale(0.99);
  transition: transform 120ms var(--ease-out), box-shadow 120ms var(--ease-out);
  box-shadow: var(--shadow-sm);
}
```

**为什么**：
- 卡片是可点击的（`cursor: pointer`），必须提供按压反馈。
- `scale(0.99)` 非常微妙——卡片比按钮大，缩放太多会分散注意力。
- `translateY(0)` 覆盖 hover 时的 `translateY(-1px)`，确保按下时卡片回到原位（下沉感）。

#### B3.2 入口动画（可选增强）

**修改——在 `.todo-center-card` 规则中增强**：
```css
.todo-center-card {
  display: flex;
  flex-direction: column;
  gap: var(--space-sm);
  padding: var(--space-md);
  background: var(--color-bg-elevated);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  box-shadow: var(--shadow-sm);
  cursor: pointer;
  transition: box-shadow 180ms var(--ease-out), transform 180ms var(--ease-out), border-color 180ms var(--ease-out);
  /* 入口动画：列表加载时卡片从下方淡入 */
  animation: card-enter 300ms var(--ease-out) both;
  animation-delay: calc(var(--card-index, 0) * 40ms);
}

@keyframes card-enter {
  from {
    opacity: 0;
    transform: translateY(8px);
  }
  to {
    opacity: 1;
    transform: translateY(0);
  }
}
```

**注意**：
- 入口动画需要列表项设置 `--card-index` CSS 变量，如果列表是用 `.map()` 渲染的，可以通过 `style={{ '--card-index': index }}` 传入（需要 TypeScript 类型扩展支持 CSS 变量）。
- 如果实现 `--card-index` 有难度，可以先不做 stagger，统一使用 `@starting-style`（见下方）。
- **更简单的方式**（推荐）：使用 `@starting-style` 纯 CSS 方案，不需要 JS：

```css
/* 需要浏览器支持 @starting-style（Chrome 117+） */
@starting-style {
  .todo-center-card {
    opacity: 0;
    transform: translateY(8px);
  }
}
```

- 考虑到兼容性，建议**暂不使用**入口动画，只做 `:active` 按压反馈。入口动画可以后续单独 PR 处理。

**验证**：
- 在事项列表中点击任意卡片，确认有轻微下沉感。
- 确认 `active` 状态不会与 `selected` 状态冲突。

---

### B4. 全局 `prefers-reduced-motion` 完善

**文件**：`frontend/src/App.css`

**当前状态**（约第 1180 行和第 3653 行）：已有两处 `prefers-reduced-motion`，但覆盖范围太小。

**修改——删除现有分散的 media query，合并为一个全局、全面的规则块，放在文件末尾（所有动画定义之后）**：

```css
/* ============================================================
   prefers-reduced-motion: 尊重用户的运动偏好
   原则：减少动画 ≠ 零动画。保留 opacity/color 辅助理解，
   移除 transform 运动、弹跳、过冲等可能引发前庭不适的效果。
   ============================================================ */
@media (prefers-reduced-motion: reduce) {
  /* 1. 全局过渡：只允许 opacity 和 color 变化 */
  *,
  *::before,
  *::after {
    animation-duration: 0.01ms !important;
    animation-iteration-count: 1 !important;
    transition-duration: 0.01ms !important;
    scroll-behavior: auto !important;
  }

  /* 2. 例外：允许 opacity 和 color 过渡，帮助状态理解 */
  .ant-btn,
  .ant-btn-primary,
  button,
  .todo-item,
  .todo-center-card,
  .tag-chip,
  .ntd-left-rail-btn,
  .ntd-left-rail-expanded-btn,
  .execution-tab,
  .history-item-compact,
  .tag-check-card,
  .fab-item-btn,
  .fab-collapse-btn {
    transition: opacity 150ms ease, color 150ms ease, background-color 150ms ease !important;
    transform: none !important;
  }

  /* 3. 禁用 keyframe 动画（保留必要的状态指示） */
  .fade-in,
  .fade-in-up,
  .slide-in-right,
  .skeleton-row,
  .conclusion-content,
  .typing-bounce,
  .chat-typing-dot,
  .ntd-logo,
  .ntd-logo::before,
  .ntd-logo::after {
    animation: none !important;
  }

  /* 4. 骨架屏在减少动画模式下直接显示静态背景 */
  .skeleton-row {
    background: var(--color-bg-hover);
    background-size: 100% 100%;
  }

  /* 5. 执行面板 tab spinner：改为静态圆点 */
  .tab-spinner {
    animation: none !important;
    border-top-color: var(--color-primary);
    opacity: 0.5;
  }
}
```

**为什么**：
- Apple Design："Reduced motion doesn't mean no feedback — it means a gentler, non-vestibular equivalent."
- `prefers-reduced-motion: reduce` 的用户可能对运动敏感，需要彻底禁用 transform 动画。
- 但保留 opacity 和 color 过渡，因为这些变化不会引发不适，且帮助用户理解状态变化。

**验证**：
- 在 macOS 系统设置中开启"减少动态效果"（系统偏好设置 → 辅助功能 → 显示 → 减少动态效果）。
- 刷新页面，确认所有 transform 动画消失，但 hover 颜色变化仍然平滑。
- 在 Chrome DevTools → Rendering → 模拟 prefers-reduced-motion: reduce 进行测试。

---

### C1. FAB 展开/收缩 stagger 动画

**文件**：`frontend/src/App.css` 和 `frontend/src/components/shell/FloatingActionButton.tsx`

#### C1.1 CSS 部分

**修改——在 `.fab-group` 规则之后（约第 1109 行后）添加**：

```css
/* FAB 按钮组 stagger 入场动画 */
.fab-group .fab-item-btn {
  opacity: 0;
  transform: scale(0.8) translateX(10px);
  animation: fab-item-enter 250ms var(--ease-out) forwards;
}

/* stagger 延迟：从上到下依次出现，间隔 40ms */
.fab-group .fab-item-btn:nth-child(2) {
  animation-delay: 40ms;
}
.fab-group .fab-item-btn:nth-child(3) {
  animation-delay: 80ms;
}

@keyframes fab-item-enter {
  from {
    opacity: 0;
    transform: scale(0.8) translateX(10px);
  }
  to {
    opacity: 1;
    transform: scale(1) translateX(0);
  }
}

/* 收缩时：反向退出 */
.fab-group.is-collapsing .fab-item-btn {
  animation: fab-item-exit 200ms var(--ease-out) forwards;
}

.fab-group.is-collapsing .fab-item-btn:nth-child(2) {
  animation-delay: 0ms;
}
.fab-group.is-collapsing .fab-item-btn:nth-child(3) {
  animation-delay: 40ms;
}

@keyframes fab-item-exit {
  from {
    opacity: 1;
    transform: scale(1) translateX(0);
  }
  to {
    opacity: 0;
    transform: scale(0.8) translateX(10px);
  }
}
```

#### C1.2 React 部分

**当前状态**：`FloatingActionButton.tsx` 中没有状态类名来标记"正在收缩"。

**修改**：

在 `FloatingActionButton.tsx` 中，修改展开状态的渲染：

```tsx
// 在组件顶部添加一个状态来标记正在进行的过渡
const [isCollapsing, setIsCollapsing] = useState(false);

const handleCollapse = useCallback(() => {
  setIsCollapsing(true);
  // 等待退出动画完成后再真正切换 collapsed 状态
  setTimeout(() => {
    setCollapsed(true);
    setIsCollapsing(false);
    try { localStorage.setItem('fab_collapsed', 'true'); } catch {}
  }, 250);
}, []);

// 展开状态渲染
return (
  <div className={`fab-group ${isCollapsing ? 'is-collapsing' : ''}`}>
    {/* ... */}
  </div>
);
```

**替代方案**（更简单，不需要 JS 状态）：
如果 React 状态修改过于复杂，可以**纯 CSS 实现**：使用 CSS 的 `:has()` 选择器或利用 `transition` 而非 `animation`。但由于 `fab-group` 在展开/收缩时元素会被 mount/unmount，纯 CSS 方案受限。

**建议**：**跳过 C1**，因为实现复杂度较高，且 FAB 的使用频率不算高（偶尔点击），当前简单的展开/收缩已足够。将精力投入到更高杠杆的改动（A1-A3、B1-B4）。

**结论**：C1 标记为 **可选，建议跳过**。

---

### C2. Dashboard Tab 切换内容过渡

**文件**：`frontend/src/App.css` 和 `frontend/src/components/Dashboard.tsx`

#### C2.1 CSS 部分

**修改——在 `App.css` 末尾添加**：

```css
/* ============================================================
   Dashboard Tab 内容切换过渡
   原则：Tab 切换时内容不应瞬间跳变，需要平滑过渡。
   使用 opacity + 轻微位移，不阻塞用户操作。
   ============================================================ */

.dashboard-tab-panel {
  animation: tab-panel-enter 250ms var(--ease-out) both;
}

@keyframes tab-panel-enter {
  from {
    opacity: 0;
    transform: translateY(4px);
  }
  to {
    opacity: 1;
    transform: translateY(0);
  }
}

/* 减少动画模式下：只保留 opacity 变化 */
@media (prefers-reduced-motion: reduce) {
  .dashboard-tab-panel {
    animation: tab-panel-enter-reduced 200ms ease both;
  }

  @keyframes tab-panel-enter-reduced {
    from { opacity: 0; }
    to   { opacity: 1; }
  }
}
```

#### C2.2 React 部分

**文件**：`frontend/src/components/Dashboard.tsx`

**当前状态**：Tab 内容通过 `Tabs` 组件的 `items` 属性传入，没有 key 变化触发的过渡。

**修改——给 Tab 内容的根元素添加 `dashboard-tab-panel` 类**：

在 `Dashboard.tsx` 中，每个 Tab 的 `children` 需要被包裹：

```tsx
// 在文件顶部添加一个包装组件
function TabPanel({ children }: { children: React.ReactNode }) {
  return <div className="dashboard-tab-panel">{children}</div>;
}

// 然后修改 tabItems 中的 children：
const tabItems = [
  {
    key: 'overview',
    label: renderLabel(AppstoreOutlined, '总览', '总览'),
    children: (
      <TabPanel>
        <OverviewTab /* ... */ />
      </TabPanel>
    ),
  },
  // ... 其他 tab 同理
];
```

**注意**：
- 需要 `import type { ReactNode } from 'react';`（如果尚未导入）。
- 由于 `TabPanel` 每次渲染都会创建新元素，key 变化时会触发 mount/unmount，从而触发 CSS 动画。

**为什么**：
- Tab 切换是"偶尔"频率，适合标准动画。
- `translateY(4px)` 非常轻微，只提供方向感，不抢眼。
- 250ms 在预算内。

**验证**：
- 切换 Dashboard 的 Tab，观察内容区域是否有轻微的淡入 + 上移效果。
- 如果 Tab 切换感觉"粘滞"，将时长降到 200ms。

---

### C3. 执行面板展开/折叠平滑过渡

**文件**：`frontend/src/App.css`

**当前状态**（约第 1656-1679 行）：
```css
.execution-panel {
  /* ... */
  transition: height 0.3s ease;
  height: 280px;
}

.execution-panel.collapsed {
  height: 40px;
}

.execution-panel.fullscreen {
  height: 100vh;
  top: 0;
  z-index: 1000;
}
```

**修改**：
```css
.execution-panel {
  /* ... */
  /* 使用更强的 ease-out + 更短的时长，让面板响应更快 */
  transition: height 250ms var(--ease-drawer), top 250ms var(--ease-drawer);
  height: 280px;
}

.execution-panel.collapsed {
  height: 40px;
}

.execution-panel.fullscreen {
  height: 100vh;
  top: 0;
  z-index: 1000;
  /* 全屏切换使用稍长时长，因为变化幅度大 */
  transition: height 350ms var(--ease-drawer), top 350ms var(--ease-drawer);
}
```

**为什么**：
- `--ease-drawer` (`cubic-bezier(0.32, 0.72, 0, 1)`) 是 iOS 抽屉风格曲线，非常适合面板类动画。
- 250ms 比原来的 300ms 更 snappy。
- 全屏模式变化幅度大，稍长一点（350ms）让过渡更自然。

**验证**：
- 点击执行面板的展开/折叠按钮，观察过渡是否更流畅。
- 点击全屏按钮，确认过渡不突兀。

---

### C4. 字体排版微调（字间距）

**文件**：`frontend/src/App.css`

**当前状态**：没有针对字体大小的字间距调整。

**修改——在 `:root` 字体变量区域（约第 76-82 行）之后添加**：

```css
/* 字体光学尺寸与字间距
   Apple Typography 原则：大字收紧字间距，小字放宽。
   固定 letter-spacing 在所有字号下都是错的。 */

/* 大标题：负字间距，让字母更紧凑 */
h1, h2, .display-title {
  letter-spacing: -0.02em;
}

/* 中等标题：略微收紧 */
h3, h4, .card-title {
  letter-spacing: -0.01em;
}

/* 正文：接近 0，稍微放宽以增加可读性 */
body, p, .card-description {
  letter-spacing: 0.01em;
}

/* 小字/标签：正字间距，防止密集 */
.todo-center-card-id,
.memorial-card-meta-row,
.log-timestamp,
.todo-center-card-time {
  letter-spacing: 0.02em;
}

/* 启用光学尺寸：浏览器根据字号自动调整字形 */
body {
  font-optical-sizing: auto;
}
```

**为什么**：
- Apple Design Typography 原则："Tracking is size-specific — never one value for all sizes."
- 大字（如标题）字母间距天然显得更大，需要负 tracking 来收紧。
- 小字需要正 tracking 来防止字符挤在一起。
- `font-optical-sizing: auto` 让字体根据显示大小自动优化字形（如果字体支持）。

**验证**：
- 打开页面，肉眼观察标题是否更紧凑、更有设计感。
- 在 DevTools 中选中标题元素，确认 `letter-spacing` 计算值。

---

### C5. Modal/Drawer 动画统一优化

**文件**：`frontend/src/App.css`

**当前状态**：项目中大量使用 Ant Design 的 `Modal` 和 `Drawer` 组件（48 个文件），但没有统一的动画风格。Ant Design 默认动画使用 `ease` 曲线，缺少自定义缓动和精细控制。

**修改——在 `App.css` 末尾（C4 之后、第四部分之前）添加**：

```css
/* ============================================================
   Modal/Drawer 动画统一优化
   原则：
   1. Modal（居中）：从 scale(0.95) + opacity(0) 进入，中心缩放
   2. Drawer（右侧）：从右侧滑入，使用 --ease-drawer 曲线
   3. Drawer（底部，移动端）：从底部滑入
   4. 出场比入场更快（退出时用户希望尽快回到工作）
   5. 遮罩层单独做 opacity 过渡，与内容动画同步
   ============================================================ */

/* ====================
   Modal 动画
   ==================== */

/* Modal 入场：从中心缩放 + 淡入 */
.ant-modal-wrap:not(.ant-modal-wrap-visible) .ant-modal {
  opacity: 0;
  transform: scale(0.95);
}

.ant-modal-wrap.ant-modal-wrap-visible .ant-modal {
  animation: ntd-modal-enter 250ms var(--ease-out) forwards;
}

/* Modal 出场：更快，反向缩放 + 淡出 */
.ant-modal-wrap.ant-modal-wrap-visible.ant-modal-closing .ant-modal {
  animation: ntd-modal-exit 200ms var(--ease-out) forwards;
}

@keyframes ntd-modal-enter {
  from {
    opacity: 0;
    transform: scale(0.95);
  }
  to {
    opacity: 1;
    transform: scale(1);
  }
}

@keyframes ntd-modal-exit {
  from {
    opacity: 1;
    transform: scale(1);
  }
  to {
    opacity: 0;
    transform: scale(0.95);
  }
}

/* Modal 遮罩层：单独的淡入淡出 */
.ant-modal-mask {
  transition: opacity 200ms var(--ease-out);
}

/* ====================
   Drawer 动画（右侧）
   ==================== */

/* Drawer 入场：从右侧滑入 */
.ant-drawer-right {
  animation: ntd-drawer-enter 300ms var(--ease-drawer) forwards;
}

/* Drawer 出场：更快，向右侧滑出 */
.ant-drawer-right.ant-drawer-closing {
  animation: ntd-drawer-exit 250ms var(--ease-drawer) forwards;
}

@keyframes ntd-drawer-enter {
  from {
    opacity: 0;
    transform: translateX(100%);
  }
  to {
    opacity: 1;
    transform: translateX(0);
  }
}

@keyframes ntd-drawer-exit {
  from {
    opacity: 1;
    transform: translateX(0);
  }
  to {
    opacity: 0;
    transform: translateX(100%);
  }
}

/* ====================
   Drawer 动画（底部，移动端）
   ==================== */

/* 底部 Drawer 入场：从底部滑入 */
.ant-drawer-bottom {
  animation: ntd-drawer-bottom-enter 300ms var(--ease-drawer) forwards;
}

/* 底部 Drawer 出场：更快，向底部滑出 */
.ant-drawer-bottom.ant-drawer-closing {
  animation: ntd-drawer-bottom-exit 250ms var(--ease-drawer) forwards;
}

@keyframes ntd-drawer-bottom-enter {
  from {
    opacity: 0;
    transform: translateY(100%);
  }
  to {
    opacity: 1;
    transform: translateY(0);
  }
}

@keyframes ntd-drawer-bottom-exit {
  from {
    opacity: 1;
    transform: translateY(0);
  }
  to {
    opacity: 0;
    transform: translateY(100%);
  }
}

/* ====================
   Drawer 遮罩层
   ==================== */

.ant-drawer-mask {
  transition: opacity 250ms var(--ease-drawer);
}

/* ====================
   prefers-reduced-motion
   ==================== */

@media (prefers-reduced-motion: reduce) {
  /* 禁用所有 transform 动画，只保留 opacity */
  .ant-modal,
  .ant-drawer-right,
  .ant-drawer-bottom {
    animation: none !important;
    opacity: 1;
    transform: none !important;
  }

  /* 遮罩层保留快速淡入淡出 */
  .ant-modal-mask,
  .ant-drawer-mask {
    transition: opacity 150ms ease;
  }
}
```

**为什么**：
- **Modal 从中心缩放**：Apple Design 原则——模态框不锚定到特定触发点，从中心出现是正确的。
- **Drawer 从边缘滑入**：空间一致性原则——进入和退出沿同一路径（右侧 → 右侧，底部 → 底部）。
- **`--ease-drawer` 曲线**：iOS 抽屉风格，比普通 `ease-out` 更平滑自然。
- **出场比入场快**：退出时用户已经完成操作，希望尽快回到工作，不需要长时间等待。
- **不从 `scale(0)` 开始**：Emil 原则——"Nothing in the real world disappears and reappears completely."，从 `scale(0.95)` 开始更自然。
- **遮罩层单独动画**：避免内容和遮罩同步动画导致的视觉跳跃。

**注意**：
- Ant Design 的 Modal 使用 `ant-modal-wrap-visible` 和 `ant-modal-closing` 类名来控制显隐状态。
- Ant Design 的 Drawer 使用 `ant-drawer-closing` 类名来标记关闭状态。
- `translateX(100%)` 和 `translateY(100%)` 使用百分比而非固定像素值，适配任意宽度/高度的 Drawer。

**验证**：
- 打开任意 Modal（如 [QuickCaptureModal](file:///Users/weibh/projects/rust/nothing-todo/frontend/src/components/QuickCaptureModal.tsx)），观察是否从中心缩放进入。
- 打开任意右侧 Drawer（如 [TodoDrawer](file:///Users/weibh/projects/rust/nothing-todo/frontend/src/components/TodoDrawer.tsx)），观察是否从右侧平滑滑入。
- 在移动端或调整窗口到小屏幕，打开底部 Drawer，观察是否从底部滑入。
- 在 `prefers-reduced-motion: reduce` 模式下，确认所有 transform 动画消失，只有 opacity 变化。

---

## 第四部分：执行顺序建议

### Phase 1（必须做，约 1 小时）
1. **A1** — 升级缓动曲线变量（影响所有后续改动）
2. **A2** — 全局按钮 `:active` 按压反馈（最高用户感知杠杆）
3. **A3** — 修复 `transition: all`（性能 + 规范）

### Phase 2（推荐做，约 1 小时）
4. **B1** — 左侧导航栏玻璃态 + 按压反馈
5. **B2** — 配置菜单 Popover 空间一致性
6. **B3** — 事项卡片按压反馈
7. **B4** — `prefers-reduced-motion` 完善

### Phase 3（可选做，约 1.5 小时）
8. **C2** — Dashboard Tab 切换过渡
9. **C3** — 执行面板展开/折叠优化
10. **C4** — 字体排版微调
11. **C5** — Modal/Drawer 动画统一优化

**注意**：Phase 1 中的 A1 必须先做，因为 A2、A3 以及后续所有改动都依赖新的 `--ease-out` 变量。

---

## 第五部分：验证清单（每完成一个改动必须检查）

- [ ] `cd frontend && npx tsc --noEmit` 零错误
- [ ] 亮色模式下所有动画正常
- [ ] 暗色模式下所有动画正常
- [ ] `prefers-reduced-motion: reduce` 模式下 transform 动画消失，opacity/color 保留
- [ ] 按钮按压反馈在所有页面可用
- [ ] 没有引入新的 `transition: all`
- [ ] 没有使用 `ease-in` 作为 UI 动画缓动
- [ ] 没有从 `scale(0)` 开始的动画
- [ ] 所有新动画时长 ≤ 300ms（营销/解释性动画除外）

---

## 第六部分：理念速查卡（执行时随时参考）

| 原则 | 一句话 | 适用场景 |
|------|--------|----------|
| 频率决策 | 高频 = 少动画，低频 = 可动画 | 每天使用 100+ 次的操作绝不加动画 |
| 即时反馈 | 按压反馈在 pointer-down，不在 click | 所有按钮、卡片、可点击元素 |
| 缓动选择 | UI 用 ease-out，对称运动用 ease-in-out | 下拉菜单、模态框、抽屉 |
| 起点规则 | 不从 scale(0) 开始，从 scale(0.95)+opacity:0 | 所有入场动画 |
| 性能铁律 | 只动画 transform 和 opacity | 永远不动画 width/height/margin/padding |
| 可打断性 | 手势动画必须随时可打断并反向 | 拖拽、滑动、抽屉 |
| 空间一致 | 进入和退出沿同一路径 | Popover、抽屉、Toast |
| 触发点 | Popover 从触发元素缩放，非中心 | 菜单、Tooltip、Dropdown |
| 减少动画 | 保留 opacity/color，移除 transform | `prefers-reduced-motion: reduce` |
| 弹性物理 | 弹簧参数 damping:1.0 默认，0.8 仅用于动量手势 | 拖拽释放、滑动翻页 |
