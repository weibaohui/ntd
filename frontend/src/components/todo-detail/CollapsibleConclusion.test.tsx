// CollapsibleConclusion 单元测试（可折叠结论，issue #652）。
//
// 本文件由原 Playwright harness spec `tests/issue-652-collapsible-conclusion.spec.ts` 迁移而来：
// 原写法用 vite dev server 服务 `/tests/issue-652-mount.html`（其引用的 mount 脚本
// issue-652-mount.ts 已丢失），把组件挂到浏览器再断言——既依赖独立的 vite 进程（make dev
// 的 18088 embedded 不服务 /tests/*），又因 mount 脚本缺失而必定超时失败。
//
// 这里直接用 @testing-library/react 在 jsdom 渲染断言。组件依赖的 LazyXMarkdown（懒加载
// markdown 渲染器）与 CopyButton（document.execCommand 复制）各自有独立关注点，且 jsdom
// 不实现 execCommand，故 mock 掉以聚焦「折叠/持久化/状态色/标题/复制接线」逻辑——这与原
// spec 的验证意图一致（原 spec 也只读 textContent/data-attr，不校验 markdown 渲染保真）。

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, fireEvent } from '@testing-library/react';
import type { MessageInstance } from 'antd/es/message/interface';

// 隔离 markdown 渲染器：直接把 content 作为纯文本渲染，便于断言内容出现/消失。
vi.mock('@/components/common/LazyXMarkdown', () => ({
  LazyXMarkdown: ({ content }: { content: string }) => <div>{content}</div>,
}));
// 隔离复制按钮：把 text 透到 data-text，onClick 触发 onCopy，规避 jsdom 不支持 execCommand。
vi.mock('@/components/CopyButton', () => ({
  CopyButton: (props: { text: string; onCopy?: () => void; 'data-testid'?: string }) => (
    <button data-testid={props['data-testid'] ?? 'conclusion-copy'} data-text={props.text} onClick={props.onCopy}>
      复制
    </button>
  ),
}));

// 待测组件在 vi.mock 之后导入，确保 mock 生效。
import { CollapsibleConclusion } from './CollapsibleConclusion';

// 长结论样本：含标题/列表/代码块，模拟真实执行结果（与原 spec 同源）。
const LONG_RESULT = `# 完成情况

1. 实现 CollapsibleConclusion 组件
2. 替换 3 个文件中的内联结论区
3. 添加折叠态 CSS

\`\`\`ts
const ok = await copyToClipboard(result);
\`\`\`

- [x] 默认展开
- [x] 折叠/展开切换
- [x] localStorage 持久化
`;

describe('CollapsibleConclusion', () => {
  // 每个用例独立 localStorage：组件按 recordId 持久化折叠态，不清理会互相串扰。
  beforeEach(() => {
    window.localStorage.clear();
  });

  it('默认展开：显示 Markdown 内容与字数统计，aria 关联正确', () => {
    const { getByTestId } = render(
      <CollapsibleConclusion result={LONG_RESULT} status="success" />,
    );

    const root = getByTestId('collapsible-conclusion');
    expect(root.getAttribute('data-collapsed')).toBe('false');

    // 内容区域可见且包含正文
    const content = getByTestId('conclusion-content');
    expect(content.textContent).toContain('完成情况');

    // toggle 含字数统计，aria 展开态 + 指向内容区 id（WAI-ARIA disclosure）
    const toggle = getByTestId('conclusion-toggle');
    expect(toggle.textContent).toContain('字');
    expect(toggle.getAttribute('aria-expanded')).toBe('true');
    expect(toggle.getAttribute('aria-controls')).toBe('conclusion-content-default');
  });

  it('点击 toggle 折叠：内容消失、aria-expanded=false', () => {
    const { getByTestId, queryByTestId } = render(
      <CollapsibleConclusion result={LONG_RESULT} status="success" recordId={1001} />,
    );

    fireEvent.click(getByTestId('conclusion-toggle'));

    expect(getByTestId('collapsible-conclusion').getAttribute('data-collapsed')).toBe('true');
    expect(getByTestId('conclusion-toggle').getAttribute('aria-expanded')).toBe('false');
    // 折叠后正文不渲染
    expect(queryByTestId('conclusion-content')).toBeNull();
  });

  it('再次点击 toggle：恢复展开', () => {
    const { getByTestId } = render(
      <CollapsibleConclusion result={LONG_RESULT} status="success" recordId={1002} />,
    );
    const toggle = getByTestId('conclusion-toggle');

    fireEvent.click(toggle);
    fireEvent.click(toggle);

    expect(getByTestId('collapsible-conclusion').getAttribute('data-collapsed')).toBe('false');
    expect(getByTestId('conclusion-content')).toBeTruthy();
  });

  it('localStorage 持久化：折叠后用同一 recordId 重新挂载仍保持折叠', () => {
    // 用固定 recordId 让 localStorage 生效；折叠后卸载再重挂，模拟刷新。
    const { getByTestId, unmount } = render(
      <CollapsibleConclusion result={LONG_RESULT} status="success" recordId={9999} />,
    );
    fireEvent.click(getByTestId('conclusion-toggle'));
    expect(getByTestId('collapsible-conclusion').getAttribute('data-collapsed')).toBe('true');
    unmount();

    // 重新挂载同一 recordId：useState 初始值读 localStorage='true' → 折叠态
    const { getByTestId: getAfterRemount } = render(
      <CollapsibleConclusion result={LONG_RESULT} status="success" recordId={9999} />,
    );
    expect(getAfterRemount('collapsible-conclusion').getAttribute('data-collapsed')).toBe('true');
  });

  it('showTitle=true：toggle 内显示「结论」标题', () => {
    const { getByTestId } = render(
      <CollapsibleConclusion result={LONG_RESULT} status="success" recordId={2001} showTitle />,
    );
    expect(getByTestId('conclusion-toggle').textContent).toContain('结论');
  });

  it('失败状态：容器 className 含 history-result-failed', () => {
    const { getByTestId } = render(
      <CollapsibleConclusion result="执行失败原因..." status="failed" recordId={3001} />,
    );
    expect(getByTestId('collapsible-conclusion').className).toMatch(/history-result-failed/);
  });

  it('复制按钮：接收完整 result 文本，点击触发成功提示', () => {
    // jsdom 不实现 document.execCommand，无法校验真实剪贴板写入；
    // 这里验证 CopyButton 拿到完整 result（data-text）且点击经 handleCopy 触发 messageApi.success。
    const messageApi = { success: vi.fn(), error: vi.fn() } as unknown as MessageInstance;
    const result = '需要复制的结论内容';
    const { getByTestId } = render(
      <CollapsibleConclusion result={result} status="success" recordId={4001} messageApi={messageApi} />,
    );

    const copy = getByTestId('conclusion-copy');
    expect(copy.getAttribute('data-text')).toBe(result);

    fireEvent.click(copy);
    expect(messageApi.success).toHaveBeenCalledWith('已复制到剪贴板');
  });
});
