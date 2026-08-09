// 执行器配置（EXECUTORS 常量及派生集/助手函数）单元测试。
//
// 本文件由原 Playwright spec `tests/kilo-executor.spec.ts` 迁移而来：原写法用 Playwright +
// Vite dev server 在浏览器里 `import('/src/types/execution.tsx')` 跑「浏览器内模块单测」，
// 依赖独立的 5173 vite 进程（make dev 的 18088 embedded 模式不服务 /src/* 模块），导致该 spec
// 长期整批失败。被测对象是纯常量与纯函数（无 DOM 副作用），完全可在 vitest 直接 import 断言，
// 无需浏览器与 vite——故迁来，消除「单测依赖 Playwright + vite」的反模式。
//
// 这些符号实际定义在 ./executors（@/types/execution 仅做 re-export 保持兼容），故直接测源模块。
// 覆盖范围与原 spec 逐条对应，不增不减；以 kilo 执行器为主线校验整组配置的正确性。

import { describe, it, expect } from 'vitest';
import {
  EXECUTORS,
  EXECUTOR_COLORS,
  RESUMABLE_EXECUTORS,
  EXECUTORS_FOR_PICKER,
  getExecutorColor,
  getExecutorOption,
  supportsResume,
} from './executors';
import type { ExecutionRecord } from '@/types/execution';

// 构造一条 kilo 执行记录的工厂：supportsResume 只读 status/executor/session_id 三个字段，
// 其余字段给出合法默认值仅为满足 ExecutionRecord 的必填约束，避免每个用例重复一大坨样板。
function makeKiloRecord(overrides: Partial<ExecutionRecord> = {}): ExecutionRecord {
  return {
    id: 1,
    todo_id: 1,
    status: 'success',
    command: 'kilo run --format json --dangerously-skip-permissions test',
    stdout: '',
    stderr: '',
    result: 'done',
    started_at: '2024-01-01T00:00:00Z',
    finished_at: '2024-01-01T00:01:00Z',
    usage: null,
    executor: 'kilo',
    model: null,
    trigger_type: 'manual',
    pid: null,
    session_id: 'ses_kilo_001',
    ...overrides,
  };
}

describe('EXECUTORS 常量', () => {
  it('包含 kilo 条目且无重复', () => {
    // 执行器 picker 依赖唯一 value；重复会让 select 出现两项导致选择错乱
    const kiloCount = EXECUTORS.filter((e) => e.value === 'kilo').length;
    expect(kiloCount).toBe(1);
  });

  it('kilo 条目 label/color/resumable 正确', () => {
    const entry = EXECUTORS.find((e) => e.value === 'kilo');
    expect(entry).toBeDefined();
    expect(entry?.label).toBe('Kilo');
    expect(entry?.color).toBe('#e67700');
    expect(entry?.resumable).toBe(true);
  });
});

describe('EXECUTOR_COLORS 常量', () => {
  it('kilo 颜色为 #e67700', () => {
    expect(EXECUTOR_COLORS['kilo']).toBe('#e67700');
  });

  it('kilo 颜色与 zhanlu/agents 不冲突（各自独立可辨）', () => {
    // 颜色相近会让看板/徽标视觉混淆，故要求互不相同
    expect(EXECUTOR_COLORS['kilo']).not.toBe(EXECUTOR_COLORS['zhanlu']);
    expect(EXECUTOR_COLORS['kilo']).not.toBe(EXECUTOR_COLORS['agents']);
  });

  it('EXECUTOR_COLORS[kilo] 与 EXECUTORS[kilo].color 一致（单一真相源）', () => {
    const entry = EXECUTORS.find((e) => e.value === 'kilo');
    expect(entry?.color).toBe(EXECUTOR_COLORS['kilo']);
  });
});

describe('RESUMABLE_EXECUTORS 派生集', () => {
  it('包含 kilo', () => {
    // supportsResume 据此集合判断，kilo 可断点续传
    expect(RESUMABLE_EXECUTORS.has('kilo')).toBe(true);
  });
});

describe('EXECUTORS_FOR_PICKER', () => {
  it('包含 kilo（kilo 非 agents）', () => {
    expect(EXECUTORS_FOR_PICKER.some((e) => e.value === 'kilo')).toBe(true);
  });

  it('不含 agents（agents 是聚合项，不应出现在单选 picker）', () => {
    expect(EXECUTORS_FOR_PICKER.some((e) => e.value === 'agents')).toBe(false);
  });
});

describe('getExecutorColor', () => {
  it('kilo 返回 #e67700', () => {
    expect(getExecutorColor('kilo')).toBe('#e67700');
  });

  it('undefined / 未知执行器返回兜底色 #999', () => {
    // 兜底色保证未登记执行器也能渲染（不报错、不空白）
    expect(getExecutorColor(undefined)).toBe('#999');
    expect(getExecutorColor('unknown_executor')).toBe('#999');
  });
});

describe('getExecutorOption', () => {
  it('kilo 返回对应条目', () => {
    const opt = getExecutorOption('kilo');
    expect(opt).toMatchObject({ value: 'kilo', label: 'Kilo', color: '#e67700' });
  });

  it('大小写无关（KILO 也能命中 kilo）', () => {
    // 用户输入/历史数据可能大小写不一，需归一匹配
    expect(getExecutorOption('KILO').value).toBe('kilo');
  });
});

describe('supportsResume', () => {
  it('kilo 成功记录带 session_id → 可恢复', () => {
    expect(supportsResume(makeKiloRecord())).toBe(true);
  });

  it('kilo 记录无 session_id → 不可恢复', () => {
    // 断点续传必须有 session 锚点
    expect(supportsResume(makeKiloRecord({ session_id: null }))).toBe(false);
  });

  it('running 态即使有 session_id 也不可恢复', () => {
    // 正在运行的任务本身占用进程，不能「恢复」（会重复拉起）
    expect(supportsResume(makeKiloRecord({ status: 'running', result: null, finished_at: null, pid: 1234 }))).toBe(false);
  });
});
