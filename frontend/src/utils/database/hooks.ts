/** Inline hook items attached to a todo. Hooks live in the `todos.hooks`
 *  column as a JSON array — there is no global hook library or rules engine.
 *
 *  The only triggers are per-target-state: each fires when the source todo
 *  transitions INTO that status. The `{{message}}` placeholder inside the
 *  target todo's prompt is filled with the source todo's most recent
 *  successful execution `result`. If the source has not run yet, the
 *  source's `prompt` is used as a fallback.
 *
 *  This matches the manual "execute with args" flow — the user writes the
 *  template by editing the target todo's prompt, hooks just supply the
 *  `{{message}}` value automatically. */

export type HookTrigger =
  | 'state_changed_to_pending'
  | 'state_changed_to_in_progress'
  | 'state_changed_to_completed'
  | 'state_changed_to_failed';

export interface TodoHookItem {
  id: number;
  trigger: HookTrigger;
  target_todo_id: number;
  skip_if_missing?: boolean;
  enabled: boolean;
}

export const HOOK_TRIGGERS: ReadonlyArray<{ value: HookTrigger; label: string }> = [
  { value: 'state_changed_to_pending', label: '状态变为待执行' },
  { value: 'state_changed_to_in_progress', label: '状态变为执行中' },
  { value: 'state_changed_to_completed', label: '状态变为已完成' },
  { value: 'state_changed_to_failed', label: '状态变为失败' },
];

const HOOK_TRIGGER_LABEL_BY_VALUE: Record<HookTrigger, string> = HOOK_TRIGGERS.reduce(
  (acc, t) => ({ ...acc, [t.value]: t.label }),
  {} as Record<HookTrigger, string>,
);

/** Look up the Chinese label for a `hook:<trigger>` trigger_type. */
export function getHookTriggerLabel(triggerType: string): string | null {
  if (!triggerType.startsWith('hook:')) return null;
  const key = triggerType.slice('hook:'.length) as HookTrigger;
  return HOOK_TRIGGER_LABEL_BY_VALUE[key] ?? triggerType;
}
