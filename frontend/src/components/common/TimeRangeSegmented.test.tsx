// TimeRangeSegmented 单元测试。
// 覆盖需求 031 §7 约定的断言面：选项渲染、showAll 开/关、value↔label 映射、
// onChange 回传（hours/null）、未知值回退。

import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { TimeRangeSegmented, TIME_RANGE_OPTIONS } from './TimeRangeSegmented';

// AntD Segmented 选中项的类名：断言选中态只依赖这个稳定 class，
// 不依赖内部 DOM 结构，降低 antd 升级导致的测试脆性。
const SELECTED_CLASS = 'ant-segmented-item-selected';

/** 取某 label 对应的 Segmented 项元素（label 文本节点的最近 item 祖先）。 */
function getItem(label: string): HTMLElement {
  const text = screen.getByText(label);
  // ant-segmented-item 是选项容器；文本在内部 span 上，向上找两层兜底。
  const item = text.closest('.ant-segmented-item');
  if (!item) throw new Error(`未找到选项容器: ${label}`);
  return item as HTMLElement;
}

describe('TimeRangeSegmented', () => {
  it('默认渲染全部 5 个时间选项且无「全部」项', () => {
    // 非 showAll 形态是历史看板场景：选项集必须与原 TIME_OPTIONS 一致，保证零行为变化。
    render(<TimeRangeSegmented value={24} onChange={() => {}} />);
    for (const o of TIME_RANGE_OPTIONS) {
      expect(screen.getByText(o.label)).toBeTruthy();
    }
    expect(screen.queryByText('全部')).toBeNull();
  });

  it('showAll 形态渲染「全部」且位于首位', () => {
    render(<TimeRangeSegmented showAll value={null} onChange={() => {}} />);
    expect(screen.getByText('全部')).toBeTruthy();
    // 首位断言：「全部」的 item 容器应是选项列表的第一个子项，
    // 保证用户视觉动线从「不过滤」开始，与需求 031 的默认全部语义一致。
    const allItem = getItem('全部');
    expect(allItem.previousElementSibling).toBeNull();
  });

  it('value=24 时 24h 项带选中态', () => {
    render(<TimeRangeSegmented value={24} onChange={() => {}} />);
    expect(getItem('24h').className).toContain(SELECTED_CLASS);
    expect(getItem('6h').className).not.toContain(SELECTED_CLASS);
  });

  it('showAll + value=null 时「全部」选中', () => {
    render(<TimeRangeSegmented showAll value={null} onChange={() => {}} />);
    expect(getItem('全部').className).toContain(SELECTED_CLASS);
  });

  it('点击 3d 回传 hours=72', () => {
    const onChange = vi.fn();
    render(<TimeRangeSegmented value={24} onChange={onChange} />);
    // 点击项内文本即可触发 Segmented 选择（label 在可点区域内）。
    fireEvent.click(screen.getByText('3d'));
    expect(onChange).toHaveBeenCalledWith(72);
  });

  it('showAll 点击「全部」回传 null', () => {
    const onChange = vi.fn();
    render(<TimeRangeSegmented showAll value={24} onChange={onChange} />);
    fireEvent.click(screen.getByText('全部'));
    expect(onChange).toHaveBeenCalledWith(null);
  });

  it('未知 value 在非 showAll 形态回退选中 24h', () => {
    // 与历史实现 `|| '24h'` 的回退行为对齐：上游传入非法 hours 时界面不能「无选中」。
    render(<TimeRangeSegmented value={999} onChange={() => {}} />);
    expect(getItem('24h').className).toContain(SELECTED_CLASS);
  });

  it('未知 value 在 showAll 形态回退选中「全部」', () => {
    // showAll 形态回退「全部」：不过滤比静默选错时间窗更安全（设计文档 §3）。
    render(<TimeRangeSegmented showAll value={999} onChange={() => {}} />);
    expect(getItem('全部').className).toContain(SELECTED_CLASS);
  });
});
