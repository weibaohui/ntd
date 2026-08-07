// PageCard 单元测试 —— 重点覆盖 062 新增的 onBack/backLabel 统一返回按钮行为。
//
// 设计依据（docs/design/062-页面头部返回与标题统一-设计.md）：
// 1. onBack 存在时，返回按钮渲染在 extra 区最右端（操作按钮之后），全站位置唯一；
// 2. backLabel 缺省为「返回列表」，目标非列表的页面（帖子页/Wiki）传「返回」；
// 3. extra 与 onBack 同时为空时不渲染 extra 容器，保持无按钮页面的现状布局。

import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { PageCard } from './PageCard';

describe('PageCard', () => {
  it('renders back button with default label when onBack is provided', () => {
    // 默认文案应为「返回列表」：大多数详情页返回目标都是列表页
    render(<PageCard title="标题" onBack={() => {}}><div>内容</div></PageCard>);
    expect(screen.getByRole('button', { name: /返回列表/i })).toBeInTheDocument();
  });

  it('renders back button with custom backLabel', () => {
    // 目标非列表（如父级详情页）时，页面传入「返回」覆盖默认文案
    render(<PageCard title="标题" onBack={() => {}} backLabel="返回"><div>内容</div></PageCard>);
    // 用正则而非精确字符串：antd Button 内嵌 icon 会影响可访问名称的精确匹配
    expect(screen.getByRole('button', { name: /返回/ })).toBeInTheDocument();
  });

  it('calls onBack when back button is clicked', () => {
    const onBack = vi.fn();
    render(<PageCard title="标题" onBack={onBack}><div>内容</div></PageCard>);
    fireEvent.click(screen.getByRole('button', { name: /返回列表/i }));
    expect(onBack).toHaveBeenCalledTimes(1);
  });

  it('does not render back button when onBack is absent', () => {
    // 列表页等顶级页面不传 onBack，不应出现返回按钮
    render(<PageCard title="标题"><div>内容</div></PageCard>);
    expect(screen.queryByRole('button', { name: /返回/i })).not.toBeInTheDocument();
  });

  it('places back button after extra content (rightmost anchor)', () => {
    // 062 核心约定：返回按钮永远在 extra 最右端，操作按钮数量变化不影响其位置
    render(
      <PageCard
        title="标题"
        extra={<button type="button">操作</button>}
        onBack={() => {}}
      >
        <div>内容</div>
      </PageCard>,
    );
    const extra = document.querySelector('.ntd-page-card-extra');
    expect(extra).not.toBeNull();
    const buttons = extra!.querySelectorAll('button');
    expect(buttons).toHaveLength(2);
    // 第一个按钮是操作按钮，最后一个必须是返回按钮
    expect(buttons[0]).toHaveTextContent('操作');
    expect(buttons[1]).toHaveTextContent('返回列表');
  });

  it('does not render extra container when both extra and onBack are absent', () => {
    // 无右侧内容时不渲染 extra 容器，避免空容器影响既有页面布局
    render(<PageCard title="标题"><div>内容</div></PageCard>);
    expect(document.querySelector('.ntd-page-card-extra')).toBeNull();
  });

  it('does not render extra container for falsy extra (false) without onBack', () => {
    // 调用方传 extra={cond && <Button/>} 且 cond=false 时，与改造前 `extra && ...` 语义一致：不渲染容器
    render(<PageCard title="标题" extra={false}><div>内容</div></PageCard>);
    expect(document.querySelector('.ntd-page-card-extra')).toBeNull();
  });

  it('renders only back button when extra is falsy (false) but onBack is provided', () => {
    // falsy extra 不产生 DOM，但 onBack 存在时容器仍需渲染以安放返回按钮
    render(<PageCard title="标题" extra={false} onBack={() => {}}><div>内容</div></PageCard>);
    const extra = document.querySelector('.ntd-page-card-extra');
    expect(extra).not.toBeNull();
    // 容器内唯一的可见元素是返回按钮（false 不产生任何节点）
    const buttons = extra!.querySelectorAll('button');
    expect(buttons).toHaveLength(1);
    expect(buttons[0]).toHaveTextContent('返回列表');
  });
});
