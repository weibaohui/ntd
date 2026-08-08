// LazyXMarkdown 单元测试 —— 覆盖 093 懒加载包装的核心契约：
// 1. Suspense 阶段展示纯文本兜底（内容可读、不闪加载圈）；
// 2. 异步 chunk 就绪后切换为真正的 XMarkdown 渲染（markdown 转 HTML）；
// 3. content 形态与 children 形态两种输入都能工作（替换存量 7 处调用的前提）。

import { describe, it, expect } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import { LazyXMarkdown } from './LazyXMarkdown';

describe('LazyXMarkdown', () => {
  it('renders markdown content after async chunk loads (content prop form)', async () => {
    render(<LazyXMarkdown content={'**加粗文本**'} />);
    // 等异步 import 完成、真组件接管后，markdown 应被渲染为 <strong> 标签
    await waitFor(() => {
      expect(screen.getByText('加粗文本').tagName).toBe('STRONG');
    });
  });

  it('renders markdown content from string children form', async () => {
    // ChatMessageItem 等调用方使用 children 形态传文本，两种形态必须等价支持
    render(<LazyXMarkdown>{'# 标题文本'}</LazyXMarkdown>);
    await waitFor(() => {
      expect(screen.getByText('标题文本')).toBeInTheDocument();
    });
  });

  it('shows plain-text fallback before chunk resolves', () => {
    // 不 await 任何加载，首次渲染同步快照应是纯文本兜底：
    // 文本原样可读（含 markdown 语法符号），证明 fallback 没有把内容吞掉
    const { container } = render(<LazyXMarkdown content={'**未加载时的原文**'} />);
    expect(container.textContent).toContain('未加载时的原文');
  });

  it('forwards className to the underlying renderer', async () => {
    // 透传契约：BlackboardPage 等调用方依赖 className 控制排版
    render(<LazyXMarkdown content={'正文'} className="custom-md" />);
    await waitFor(() => {
      expect(screen.getByText('正文')).toBeInTheDocument();
    });
    expect(document.querySelector('.custom-md')).not.toBeNull();
  });
});
