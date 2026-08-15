// antd Segmented 选项定位辅助（spec 共用）。
//
// 背景：本仓库多个 Segmented 是「图标 + title」模式（如环路页视图切换器），antd 把
// 选项 label 渲染在 <div class="ant-segmented-item-label" title="..."> 上，radio input
// 视觉隐藏、其可访问名是图标名而非 label——getByRole('radio', { name: '看板' }) 定位不到。
//
// `.ant-segmented-item-label[title="..."]` 是验证过的可行定位（DOM 结构在
// antd v5 稳定），但散落各 spec 重复 5+ 处；集中到本 helper：
// - 单点维护：antd 升级若调整 DOM，只改这里；
// - 语义收口：调用处读 `segmentedOption(page, '看板')` 意图明确。

import { type Locator, type Page } from '@playwright/test';

/**
 * 按 title 定位 antd Segmented 选项的 label 节点（可直接 click）。
 *
 * @param page  Playwright Page
 * @param title 选项的 title 属性值（图标模式下与选项文案一致）
 * @param scope 可选的父容器 Locator（同页多个 Segmented 时用它限定范围，
 *              避免不同切换器的同名选项误命中）
 */
export function segmentedOption(page: Page, title: string, scope?: Locator): Locator {
  // scope 未传时退化为整页定位；title 精确匹配防「列表」误中「列表视图」这类前缀撞车。
  const root = scope ?? page.locator('body');
  return root.locator(`.ant-segmented-item-label[title="${title}"]`);
}
