import { describe, expect, it } from 'vitest';
import { formatProcessText } from '@/utils/processText';

describe('formatProcessText', () => {
  it('完整字段时输出 #id-名称-版本', () => {
    expect(formatProcessText(101, '标准需求交付工艺', '1.2.0')).toBe('#101-标准需求交付工艺-1.2.0');
  });

  it('name 缺失时回退 #id，version 缺失时用 — 占位', () => {
    expect(formatProcessText(101, null, null)).toBe('#101-#101-—');
    expect(formatProcessText(101, '  ', '')).toBe('#101-#101-—');
  });

  it('id 缺失时返回 -', () => {
    expect(formatProcessText(null, '标准工艺', '1.0.0')).toBe('-');
    expect(formatProcessText(undefined, '标准工艺', '1.0.0')).toBe('-');
  });
});
