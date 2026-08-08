// PromptMdField 单元测试。
// 覆盖（需求 046 验收标准对应）：
// - 渲染：MD 编辑器出现且受控值透传到内部 textarea；
// - 编辑：onChange 原样透传新文本；
// - 参数条：不传 params 时不渲染；点击参数在光标处插入（jsdom 下 textarea 存在，走光标路径）；
// - 扩展位：extraActions 渲染且此时不显示「可用参数」标签；
// - buildAppendedText 纯函数：空值 / 末尾有换行 / 末尾无换行三条边界。
// mock 掉 useTheme：MdEditor 内部只消费 themeMode，mock 后无需包 ThemeProvider，保持用例聚焦。

import { describe, it, expect, vi } from 'vitest';
// 093：MdEditor 改为懒加载后，编辑器 DOM 不再随首次 render 同步出现，
// 三个依赖编辑器节点的用例统一改为 waitFor 等 Suspense 就绪后再断言。
import { render, screen, fireEvent, waitFor } from '@testing-library/react';

// mock useTheme，返回固定亮色模式，避免 ThemeProvider 依赖与 jsdom matchMedia 缺失问题
vi.mock('@/hooks/useTheme', () => ({
  useTheme: () => ({ themeMode: 'light' }),
}));

import { PromptMdField, buildAppendedText } from './PromptMdField';

// 测试用参数集：与生产参数同构（key 为插入文本，desc 为 Tooltip 说明）
const TEST_PARAMS = [{ key: '{{original_output}}', desc: '执行输出' }];

describe('PromptMdField 渲染与编辑', () => {
  it('渲染 MD 编辑器且受控值透传到内部 textarea', async () => {
    const { container } = render(
      <PromptMdField value="初始提示词" onChange={() => {}} />,
    );
    // .w-md-editor 是 @uiw/react-md-editor 的根容器 class，作为「已换成 MD 控件」的断言锚点；
    // 093 懒加载后需等待动态 import 解析完成才会出现；
    // jsdom 下首载 vendor chunk 实测约 1s，超出 waitFor 默认 1s 上限，显式放宽到 10s
    await waitFor(
      () => expect(container.querySelector('.w-md-editor')).not.toBeNull(),
      { timeout: 10000 },
    );
    // 受控值必须出现在内部 textarea 上，保证既有 prompt 文本原样回显
    const textarea = container.querySelector('textarea');
    expect(textarea).not.toBeNull();
    expect(textarea!.value).toBe('初始提示词');
  });

  it('编辑内容时 onChange 原样透传新文本', async () => {
    const onChange = vi.fn();
    const { container } = render(
      <PromptMdField value="" onChange={onChange} />,
    );
    // 093：懒加载下 textarea 异步就绪，先 waitFor 再取节点触发输入；
    // 10s 超时理由同上（jsdom 首载 vendor chunk ~1s，留余量防 CI 抖动）
    await waitFor(
      () => expect(container.querySelector('textarea')).not.toBeNull(),
      { timeout: 10000 },
    );
    const textarea = container.querySelector('textarea');
    // 模拟用户输入：fireEvent.change 触发 @uiw 内部 onChange → 组件 onChange
    fireEvent.change(textarea!, { target: { value: '新内容' } });
    expect(onChange).toHaveBeenCalledWith('新内容');
  });
});

describe('PromptMdField 参数条', () => {
  it('不传 params 且不传 extraActions 时不渲染参数条', () => {
    render(<PromptMdField value="abc" onChange={() => {}} />);
    // 「提示词」字段场景：保持现状无参数条，断言标签不存在
    expect(screen.queryByText('可用参数:')).toBeNull();
  });

  it('点击参数在光标处插入而非尾部追加', async () => {
    const onChange = vi.fn();
    const { container } = render(
      <PromptMdField
        value="hello world"
        onChange={onChange}
        params={TEST_PARAMS}
      />,
    );
    // 093：懒加载下 textarea 异步就绪，先 waitFor 再设置光标位置（10s 超时理由同上）
    await waitFor(
      () => expect(container.querySelector('textarea')).not.toBeNull(),
      { timeout: 10000 },
    );
    const textarea = container.querySelector('textarea');
    // 把光标移到 "hello " 与 "world" 之间（index 6），模拟用户在文本中间点击参数
    textarea!.selectionStart = 6;
    textarea!.selectionEnd = 6;
    fireEvent.click(screen.getByText('{{original_output}}'));
    // 光标路径：onChange 收到切片插入后的完整文本
    expect(onChange).toHaveBeenCalledWith('hello {{original_output}}world');
  });

  it('extraActions 渲染且不显示「可用参数」标签', () => {
    render(
      <PromptMdField
        value=""
        onChange={() => {}}
        extraActions={<button>使用默认值</button>}
      />,
    );
    // 扩展位内容（如 DefaultReviewPromptButton）必须渲染出来
    expect(screen.getByText('使用默认值')).toBeInTheDocument();
    // 没有参数时不渲染参数标签，避免空标签造成视觉噪音
    expect(screen.queryByText('可用参数:')).toBeNull();
  });
});

describe('buildAppendedText 尾部追加纯函数', () => {
  it('空文本直接返回插入内容', () => {
    // 空串不需要任何分隔符，避免产生前导换行
    expect(buildAppendedText('', '{{x}}')).toBe('{{x}}');
  });

  it('末尾有换行时不重复补换行', () => {
    // 防止多次追加累积空行
    expect(buildAppendedText('abc\n', '{{x}}')).toBe('abc\n{{x}}');
  });

  it('末尾无换行时先补换行再追加', () => {
    // 参数独立成行，避免粘连破坏 prompt 语义
    expect(buildAppendedText('abc', '{{x}}')).toBe('abc\n{{x}}');
  });
});
