// execDurationSec / formatExecTimeInfo 的单元测试。
// 覆盖：正常耗时、进行中（无结束时间）、非法时间、负耗时、缺失开始时间。
import { describe, expect, it } from 'vitest';
import { execDurationSec, formatExecTimeInfo } from './helpers';

describe('execDurationSec', () => {
  it('test_execDurationSec_正常起止时间_返回秒数', () => {
    // 12:15:01 → 12:16:55 相差 114 秒，验证基本减法与毫秒截断。
    expect(execDurationSec('2026-07-04T12:15:01Z', '2026-07-04T12:16:55Z')).toBe(114);
  });

  it('test_execDurationSec_缺少结束时间_返回null', () => {
    // 进行中的执行没有 finished_at，应返回 null 让 UI 隐藏耗时。
    expect(execDurationSec('2026-07-04T12:15:01Z', null)).toBeNull();
  });

  it('test_execDurationSec_缺少开始时间_返回null', () => {
    expect(execDurationSec(undefined, '2026-07-04T12:16:55Z')).toBeNull();
  });

  it('test_execDurationSec_非法时间字符串_返回null', () => {
    // 后端异常数据不应让 NaN 传播到 UI。
    expect(execDurationSec('not-a-date', '2026-07-04T12:16:55Z')).toBeNull();
  });

  it('test_execDurationSec_结束早于开始_返回null', () => {
    // 时钟回拨等异常产生的负耗时不展示。
    expect(execDurationSec('2026-07-04T12:16:55Z', '2026-07-04T12:15:01Z')).toBeNull();
  });
});

describe('formatExecTimeInfo', () => {
  it('test_formatExecTimeInfo_已结束_包含开始时间与耗时', () => {
    const text = formatExecTimeInfo('2026-07-04T12:15:01Z', '2026-07-04T12:16:55Z');
    expect(text).toContain('开始');
    // formatDurationSec 对 >=60s 只保留分钟粒度：114s → 1m，与项目其他耗时展示口径一致。
    expect(text).toContain('耗时 1m');
  });

  it('test_formatExecTimeInfo_进行中_只有开始时间不显示耗时', () => {
    const text = formatExecTimeInfo('2026-07-04T12:15:01Z', undefined);
    expect(text).toContain('开始');
    expect(text).not.toContain('耗时');
  });

  it('test_formatExecTimeInfo_无开始时间_返回空串', () => {
    // 调用方依据空串跳过整个时间区渲染。
    expect(formatExecTimeInfo(null, null)).toBe('');
  });
});
