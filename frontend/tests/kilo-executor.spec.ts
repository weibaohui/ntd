/**
 * Kilo executor – execution.tsx tests
 *
 * Verifies that the Kilo executor entry is correctly configured in:
 * - EXECUTORS constant (value, label, color, resumable flag)
 * - EXECUTOR_COLORS constant
 * - RESUMABLE_EXECUTORS derived set
 * - EXECUTORS_FOR_PICKER (should include kilo; it is not agents)
 * - getExecutorColor() helper
 * - getExecutorOption() helper
 * - supportsResume() logic based on resumable flag
 *
 * These tests use Playwright page.evaluate() to import the live module
 * from the Vite dev server, matching the pattern used elsewhere in this
 * test suite (hook-pre-trigger.spec.ts, issue-648-command-extractor.spec.ts).
 */

import { test, expect } from '@playwright/test';

const DEV_URL = process.env.E2E_BASE_URL || 'http://localhost:5173';

// ── EXECUTORS constant ──────────────────────────────────────────────────

test('EXECUTORS contains a kilo entry', async ({ page }) => {
  await page.goto(DEV_URL);

  const kiloEntry = await page.evaluate(async () => {
    const mod = await import('/src/types/execution.tsx');
    return mod.EXECUTORS.find((e: { value: string }) => e.value === 'kilo');
  });

  expect(kiloEntry).toBeDefined();
});

test('EXECUTORS kilo entry has correct label', async ({ page }) => {
  await page.goto(DEV_URL);

  const label = await page.evaluate(async () => {
    const mod = await import('/src/types/execution.tsx');
    const entry = mod.EXECUTORS.find((e: { value: string }) => e.value === 'kilo');
    return entry?.label;
  });

  expect(label).toBe('Kilo');
});

test('EXECUTORS kilo entry has correct color #e67700', async ({ page }) => {
  await page.goto(DEV_URL);

  const color = await page.evaluate(async () => {
    const mod = await import('/src/types/execution.tsx');
    const entry = mod.EXECUTORS.find((e: { value: string }) => e.value === 'kilo');
    return entry?.color;
  });

  expect(color).toBe('#e67700');
});

test('EXECUTORS kilo entry has resumable: true', async ({ page }) => {
  await page.goto(DEV_URL);

  const resumable = await page.evaluate(async () => {
    const mod = await import('/src/types/execution.tsx');
    const entry = mod.EXECUTORS.find((e: { value: string }) => e.value === 'kilo');
    return entry?.resumable;
  });

  expect(resumable).toBe(true);
});

// ── EXECUTOR_COLORS constant ────────────────────────────────────────────

test('EXECUTOR_COLORS has kilo with color #e67700', async ({ page }) => {
  await page.goto(DEV_URL);

  const color = await page.evaluate(async () => {
    const mod = await import('/src/types/execution.tsx');
    return mod.EXECUTOR_COLORS['kilo'];
  });

  expect(color).toBe('#e67700');
});

test('EXECUTOR_COLORS kilo color matches EXECUTORS kilo color', async ({ page }) => {
  await page.goto(DEV_URL);

  const { executorsColor, colorsMapColor } = await page.evaluate(async () => {
    const mod = await import('/src/types/execution.tsx');
    const entry = mod.EXECUTORS.find((e: { value: string }) => e.value === 'kilo');
    return {
      executorsColor: entry?.color,
      colorsMapColor: mod.EXECUTOR_COLORS['kilo'],
    };
  });

  expect(executorsColor).toBe(colorsMapColor);
});

// ── RESUMABLE_EXECUTORS derived set ────────────────────────────────────

test('RESUMABLE_EXECUTORS contains kilo', async ({ page }) => {
  await page.goto(DEV_URL);

  const hasKilo = await page.evaluate(async () => {
    const mod = await import('/src/types/execution.tsx');
    return mod.RESUMABLE_EXECUTORS.has('kilo');
  });

  expect(hasKilo).toBe(true);
});

// ── EXECUTORS_FOR_PICKER ────────────────────────────────────────────────

test('EXECUTORS_FOR_PICKER contains kilo (kilo is not agents)', async ({ page }) => {
  await page.goto(DEV_URL);

  const hasKilo = await page.evaluate(async () => {
    const mod = await import('/src/types/execution.tsx');
    return mod.EXECUTORS_FOR_PICKER.some((e: { value: string }) => e.value === 'kilo');
  });

  expect(hasKilo).toBe(true);
});

test('EXECUTORS_FOR_PICKER does not contain agents', async ({ page }) => {
  await page.goto(DEV_URL);

  const hasAgents = await page.evaluate(async () => {
    const mod = await import('/src/types/execution.tsx');
    return mod.EXECUTORS_FOR_PICKER.some((e: { value: string }) => e.value === 'agents');
  });

  expect(hasAgents).toBe(false);
});

// ── getExecutorColor() helper ───────────────────────────────────────────

test('getExecutorColor("kilo") returns #e67700', async ({ page }) => {
  await page.goto(DEV_URL);

  const color = await page.evaluate(async () => {
    const mod = await import('/src/types/execution.tsx');
    return mod.getExecutorColor('kilo');
  });

  expect(color).toBe('#e67700');
});

test('getExecutorColor(undefined) returns fallback #999', async ({ page }) => {
  await page.goto(DEV_URL);

  const color = await page.evaluate(async () => {
    const mod = await import('/src/types/execution.tsx');
    return mod.getExecutorColor(undefined);
  });

  expect(color).toBe('#999');
});

test('getExecutorColor("unknown_executor") returns fallback #999', async ({ page }) => {
  await page.goto(DEV_URL);

  const color = await page.evaluate(async () => {
    const mod = await import('/src/types/execution.tsx');
    return mod.getExecutorColor('unknown_executor');
  });

  expect(color).toBe('#999');
});

// ── getExecutorOption() helper ──────────────────────────────────────────

test('getExecutorOption("kilo") returns kilo entry', async ({ page }) => {
  await page.goto(DEV_URL);

  const option = await page.evaluate(async () => {
    const mod = await import('/src/types/execution.tsx');
    const opt = mod.getExecutorOption('kilo');
    return { value: opt.value, label: opt.label, color: opt.color };
  });

  expect(option.value).toBe('kilo');
  expect(option.label).toBe('Kilo');
  expect(option.color).toBe('#e67700');
});

test('getExecutorOption("KILO") is case-insensitive', async ({ page }) => {
  await page.goto(DEV_URL);

  const value = await page.evaluate(async () => {
    const mod = await import('/src/types/execution.tsx');
    return mod.getExecutorOption('KILO').value;
  });

  expect(value).toBe('kilo');
});

// ── supportsResume() function ───────────────────────────────────────────

test('supportsResume returns true for kilo record with session_id', async ({ page }) => {
  await page.goto(DEV_URL);

  const result = await page.evaluate(async () => {
    const mod = await import('/src/types/execution.tsx');
    const record = {
      id: 1,
      todo_id: 1,
      status: 'success' as const,
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
    };
    return mod.supportsResume(record);
  });

  expect(result).toBe(true);
});

test('supportsResume returns false for kilo record without session_id', async ({ page }) => {
  await page.goto(DEV_URL);

  const result = await page.evaluate(async () => {
    const mod = await import('/src/types/execution.tsx');
    const record = {
      id: 1,
      todo_id: 1,
      status: 'success' as const,
      command: 'kilo run',
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
      session_id: null,
    };
    return mod.supportsResume(record);
  });

  expect(result).toBe(false);
});

test('supportsResume returns false for running kilo record even with session_id', async ({ page }) => {
  await page.goto(DEV_URL);

  const result = await page.evaluate(async () => {
    const mod = await import('/src/types/execution.tsx');
    const record = {
      id: 1,
      todo_id: 1,
      status: 'running' as const,
      command: 'kilo run',
      stdout: '',
      stderr: '',
      result: null,
      started_at: '2024-01-01T00:00:00Z',
      finished_at: null,
      usage: null,
      executor: 'kilo',
      model: null,
      trigger_type: 'manual',
      pid: 1234,
      session_id: 'ses_kilo_002',
    };
    return mod.supportsResume(record);
  });

  expect(result).toBe(false);
});

// ── Boundary / regression tests ─────────────────────────────────────────

test('kilo color does not conflict with zhanlu or agents colors', async ({ page }) => {
  await page.goto(DEV_URL);

  const colors = await page.evaluate(async () => {
    const mod = await import('/src/types/execution.tsx');
    return {
      kilo: mod.EXECUTOR_COLORS['kilo'],
      zhanlu: mod.EXECUTOR_COLORS['zhanlu'],
      agents: mod.EXECUTOR_COLORS['agents'],
    };
  });

  // Each executor should have a distinct color to avoid visual confusion
  expect(colors.kilo).not.toBe(colors.zhanlu);
  expect(colors.kilo).not.toBe(colors.agents);
  expect(colors.kilo).toBe('#e67700');
});

test('EXECUTORS list does not contain duplicate kilo entries', async ({ page }) => {
  await page.goto(DEV_URL);

  const kiloCount = await page.evaluate(async () => {
    const mod = await import('/src/types/execution.tsx');
    return mod.EXECUTORS.filter((e: { value: string }) => e.value === 'kilo').length;
  });

  expect(kiloCount).toBe(1);
});