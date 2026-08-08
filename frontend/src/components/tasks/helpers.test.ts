// execDurationSec / formatExecTimeInfo / visibleTaskTabs / resolveTaskActiveTab 的单元测试。
// 覆盖：正常耗时、进行中（无结束时间）、非法时间、负耗时、缺失开始时间；
// 以及 Tab 显隐按执行方式分流、URL 偏好命中/被隐藏/非法时的回退。
import { describe, expect, it } from 'vitest';
import { execDurationSec, formatExecTimeInfo, visibleTaskTabs, resolveTaskActiveTab } from './helpers';

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

describe('visibleTaskTabs', () => {
  it('test_visibleTaskTabs_委派任务_隐藏dag与exec', () => {
    // 委派任务无 loop_id，dag/exec 只会渲染空状态，故隐藏；只留概览与讨论。
    expect(visibleTaskTabs('delegate')).toEqual(['overview', 'discussion']);
  });

  it('test_visibleTaskTabs_工艺环路_展示全部四个Tab', () => {
    // 工艺环路绑定 loop_id，dag/exec 有真实数据，全展示。
    expect(visibleTaskTabs('loop')).toEqual(['overview', 'dag', 'exec', 'discussion']);
  });

  it('test_visibleTaskTabs_无executionMode字段_按全部展示', () => {
    // 历史无 execution_mode 的旧任务（DB 默认 loop）按非委派口径处理，避免误隐藏。
    expect(visibleTaskTabs(undefined)).toEqual(['overview', 'dag', 'exec', 'discussion']);
  });
});

describe('resolveTaskActiveTab', () => {
  it('test_resolveTaskActiveTab_偏好命中可见集合_直接采纳', () => {
    // 工艺环路下 ?tab=exec 可见，直接用。
    expect(resolveTaskActiveTab('exec', 'loop')).toBe('exec');
  });

  it('test_resolveTaskActiveTab_委派任务偏好dag_回退讨论', () => {
    // 委派任务隐藏 dag，残留 ?tab=dag 链接不能让 Tabs 落到无选中态空白，回退默认讨论。
    expect(resolveTaskActiveTab('dag', 'delegate')).toBe('discussion');
  });

  it('test_resolveTaskActiveTab_委派任务偏好exec_回退讨论', () => {
    expect(resolveTaskActiveTab('exec', 'delegate')).toBe('discussion');
  });

  it('test_resolveTaskActiveTab_委派任务偏好discussion_命中', () => {
    // 讨论在委派任务下可见，正常采纳。
    expect(resolveTaskActiveTab('discussion', 'delegate')).toBe('discussion');
  });

  it('test_resolveTaskActiveTab_工艺环路偏好被省略_回退概览', () => {
    // 无 ?tab=（如直接点进详情）按默认进概览。
    expect(resolveTaskActiveTab(undefined, 'loop')).toBe('overview');
  });

  it('test_resolveTaskActiveTab_偏好非TabKey_回退概览', () => {
    // ?tab=garbage 不是合法 Tab，不应被采纳，回退默认。
    expect(resolveTaskActiveTab('garbage', 'loop')).toBe('overview');
  });
});
