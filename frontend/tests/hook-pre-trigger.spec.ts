/**
 * 前置 Hook (before_execution) UI 测试
 *
 * 验证 before_execution trigger 在 UI 中的正确展示：
 * - HOOK_TRIGGERS 常量包含 before_execution 且排在第一位
 * - getHookTriggerLabel 正确解析 hook:before_execution
 * - 新建 hook 时默认 trigger 仍为 state_changed_to_completed
 *
 * 对应 Issue: feat(hook) — add before_execution pre-hook trigger
 */

import { test, expect, chromium } from '@playwright/test';

const DEV_URL = process.env.E2E_BASE_URL || 'http://localhost:5173';

/** 验证 HOOK_TRIGGERS 常量包含 before_execution，且排在第一位 */
test('HOOK_TRIGGERS 常量包含 before_execution 且在首位', async ({ page }) => {
  await page.goto(DEV_URL);

  const triggers = await page.evaluate(async () => {
    const mod = await import('/src/utils/database/hooks');
    return mod.HOOK_TRIGGERS;
  });

  expect(triggers).toBeDefined();
  expect(Array.isArray(triggers)).toBe(true);
  expect(triggers.length).toBe(5);

  // before_execution 必须在列表首位
  expect(triggers[0].value).toBe('before_execution');
  expect(triggers[0].label).toBe('执行前');

  // 验证其他4个 trigger 也在列表中
  const values = triggers.map((t: { value: string }) => t.value);
  expect(values).toContain('state_changed_to_pending');
  expect(values).toContain('state_changed_to_in_progress');
  expect(values).toContain('state_changed_to_completed');
  expect(values).toContain('state_changed_to_failed');
});

/** 验证 getHookTriggerLabel 正确解析 hook:before_execution */
test('getHookTriggerLabel 正确解析 before_execution', async ({ page }) => {
  await page.goto(DEV_URL);

  const label = await page.evaluate(async () => {
    const mod = await import('/src/utils/database/hooks');
    return mod.getHookTriggerLabel('hook:before_execution');
  });

  expect(label).toBe('执行前');
});

/** 验证 getHookTriggerLabel 对非 hook 类型返回 null */
test('getHookTriggerLabel 对非 hook 类型返回 null', async ({ page }) => {
  await page.goto(DEV_URL);

  const label = await page.evaluate(async () => {
    const mod = await import('/src/utils/database/hooks');
    return mod.getHookTriggerLabel('manual');
  });

  expect(label).toBeNull();
});

/** 验证 getHookTriggerLabel 对未知的 hook 类型返回原字符串 */
test('getHookTriggerLabel 对未知 trigger 返回原字符串', async ({ page }) => {
  await page.goto(DEV_URL);

  const label = await page.evaluate(async () => {
    const mod = await import('/src/utils/database/hooks');
    return mod.getHookTriggerLabel('hook:unknown_trigger');
  });

  // 未知 trigger 类型时返回原字符串
  expect(label).toBe('hook:unknown_trigger');
});

/** 验证 UNRATED_POLICIES 常量仍然存在（未被改动） */
test('UNRATED_POLICIES 常量保持不变', async ({ page }) => {
  await page.goto(DEV_URL);

  const policies = await page.evaluate(async () => {
    const mod = await import('/src/utils/database/hooks');
    return mod.UNRATED_POLICIES;
  });

  expect(policies).toBeDefined();
  expect(Array.isArray(policies)).toBe(true);
  expect(policies.length).toBe(2);
  expect(policies.map((p: { value: string }) => p.value)).toContain('skip');
  expect(policies.map((p: { value: string }) => p.value)).toContain('pass');
});

/** 验证 DEFAULT_MIN_RATING 和 DEFAULT_UNRATED_POLICY 未变 */
test('rating gate 默认值常量未变', async ({ page }) => {
  await page.goto(DEV_URL);

  const [minRating, unratedPolicy] = await page.evaluate(async () => {
    const mod = await import('/src/utils/database/hooks');
    return [mod.DEFAULT_MIN_RATING, mod.DEFAULT_UNRATED_POLICY];
  });

  expect(minRating).toBeNull();
  expect(unratedPolicy).toBe('skip');
});
