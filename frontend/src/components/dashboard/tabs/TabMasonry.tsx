// 各 Tab 共用的卡片瀑布流容器。
//
// 背景:Tab 化前 Dashboard 把全站 24 张卡塞进一个 Masonry,信息过载;
// 拆 Tab 后每个 Tab 内部仍是瀑布流,但范围从「全站」缩小到「本域 5-9 卡」,
// 视觉密度回到可读区间。列数/gutter 配置与原 Dashboard 完全一致,
// 避免破坏既有响应式断点(xs 单列、md 起双列、xl 三列)。
import type { ReactNode } from 'react';
import { Masonry } from 'antd';

// 单个瀑布流卡片的描述:key 用于 React diff 稳定性,render 产出实际卡片内容。
// 刻意用 () => ReactNode 而非直接 ReactNode,是为了和原 Dashboard 的 panels 结构
// 完全对齐——Masonry 的 itemRender 只在布局确定后才调用 render,避免无谓的重渲染。
export interface PanelItem {
  key: string;
  render: () => ReactNode;
}

interface TabMasonryProps {
  panels: PanelItem[];
}

export function TabMasonry({ panels }: TabMasonryProps) {
  return (
    <Masonry
      columns={{ xs: 1, sm: 1, md: 2, lg: 2, xl: 3 }}
      gutter={[16, 16]}
      items={panels.map((p) => ({ key: p.key, data: p }))}
      itemRender={(item) => item.data.render()}
      fresh
    />
  );
}
