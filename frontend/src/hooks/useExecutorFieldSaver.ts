/**
 * useExecutorFieldSaver — 执行器行内字段保存收敛 hook（096-W4-4-3 产物，本项核心实证）。
 *
 * 原 ExecutorsPanel 主组件有 5 处「保存某执行器某字段」的同构块（enabled 开关 / path /
 * session_dir / default_model 输入 / default_model 下拉），每处都重复同一套骨架：
 *   setSavingExecutor(name) → db.updateExecutor(name, patch) → setExecutors(map 替换)
 *   → catch message.error → finally setSavingExecutor(null)
 * 其中 3 处 Input 还各带 onBlur + onPressEnter(→blur) 双触发样板。
 *
 * 本 hook 收敛两件事：
 * 1. saveExecutorField(name, patch) —— 上述 5 处共用的保存骨架塌成一个函数；
 *    Switch/Select 等 onChange 型控件直接调它。
 * 2. inlineFieldSave(name, currentValue, onSave) —— 把 Input 的 onBlur(去空格+未改不存)
 *    + onPressEnter(→blur) 双触发样板封装成一个普通工厂，返回 {onBlur, onPressEnter, saving}。
 *    注意：它是闭包工厂而非 React hook（内部不调 useState/useEffect），故可在表格列 render
 *    里逐行调用而不违反 Rules of Hooks。
 *
 * 设计取舍：savingExecutor 保留原 pending-by-name 语义（`string | null`，同名互斥 loading），
 * 不强行 useReducer 化——行为零风险优先（见 doc 101 风险节）。
 */

import { useCallback, useState } from 'react';
import { App } from 'antd';
import type React from 'react';
import type { ExecutorConfig } from '@/types';
import * as db from '@/utils/database';

/** updateExecutor 接受的字段补丁（与 db/skills.ts 的 data 形参同构，单一事实源）。 */
type ExecutorPatch = {
  path?: string;
  enabled?: boolean;
  display_name?: string;
  session_dir?: string;
  default_model?: string;
};

/** inlineFieldSave 返回的双触发处理器三件套。 */
interface InlineFieldSaveHandlers {
  /** 失焦保存：去空格后与当前值比较，未改不存，改动则交给 onSave。 */
  onBlur: (e: React.FocusEvent<HTMLInputElement>) => void;
  /** 回车触发 blur → 走 onBlur 保存路径（双触发收敛到一处）。 */
  onPressEnter: (e: React.KeyboardEvent<HTMLInputElement>) => void;
  /** 该执行器是否正在保存（savingExecutor === name），供需要 loading 态的控件用。 */
  saving: boolean;
}

export interface UseExecutorFieldSaverReturn {
  /** 当前正在保存的执行器名（同名互斥 loading）；null = 空闲。 */
  savingExecutor: string | null;
  /**
   * 保存某执行器的字段补丁。返回更新后的完整配置；失败返回 null。
   * 调用方据返回值决定是否做后续副作用（如改路径后清检测状态）。
   */
  saveExecutorField: (name: string, patch: ExecutorPatch) => Promise<ExecutorConfig | null>;
  /**
   * 为 Input 型行内字段构造 onBlur/onPressEnter 双触发处理器（含未改不存守卫）。
   * onSave 承担真正的保存（通常调 saveExecutorField + 任意字段专属副作用）。
   */
  inlineFieldSave: (
    name: string,
    currentValue: string,
    onSave: (newValue: string) => Promise<void>,
  ) => InlineFieldSaveHandlers;
}

/**
 * @param replaceExecutor 把更新后的配置写回列表（由 useExecutorAdmin 提供，跨 hook 单一入口）。
 */
export function useExecutorFieldSaver(
  replaceExecutor: (name: string, updated: ExecutorConfig) => void,
): UseExecutorFieldSaverReturn {
  const { message } = App.useApp();
  const [savingExecutor, setSavingExecutor] = useState<string | null>(null);

  /**
   * 5 处同构保存块的共用骨架：置 saving → updateExecutor → 回写列表 → 失败提示 → 清 saving。
   * 返回更新后的配置（成功）或 null（失败），让调用方挂接字段专属副作用（如清检测状态）。
   */
  const saveExecutorField = useCallback(
    async (name: string, patch: ExecutorPatch): Promise<ExecutorConfig | null> => {
      setSavingExecutor(name);
      try {
        const updated = await db.updateExecutor(name, patch);
        // 回写列表项，使前端缓存与后端一致；用 replaceExecutor 统一入口避免两处 map-replace 漂移。
        replaceExecutor(name, updated);
        return updated;
      } catch (err: unknown) {
        message.error('保存失败: ' + (err instanceof Error ? err.message : String(err)));
        return null;
      } finally {
        setSavingExecutor(null);
      }
    },
    [message, replaceExecutor],
  );

  /**
   * 为 Input 型行内字段封装 onBlur + onPressEnter(→blur) 双触发。
   * 闭包工厂（非 hook）：读取当前 savingExecutor 算 saving，关闭 over 调用方传入的 onSave。
   * 未改动（去空格后与 currentValue 相等）直接返回，不发请求、不闪 saving。
   */
  const inlineFieldSave = useCallback(
    (
      name: string,
      currentValue: string,
      onSave: (newValue: string) => Promise<void>,
    ): InlineFieldSaveHandlers => ({
      onBlur: async (e: React.FocusEvent<HTMLInputElement>) => {
        // 去空格：用户粘贴的 " /x " 与 "/x" 应当等价。
        const newValue = e.target.value.trim();
        // 未改动不保存——避免每次 blur 都打无谓请求并令 saving 闪烁。
        if (newValue === currentValue) return;
        await onSave(newValue);
      },
      // 回车等价失焦：触发 blur 即走上面的 onBlur 保存路径，双触发收敛到一处。
      onPressEnter: (e: React.KeyboardEvent<HTMLInputElement>) => {
        (e.target as HTMLInputElement).blur();
      },
      // savingExecutor === name：同名互斥 loading（该执行器有任一保存在进行即亮）。
      saving: savingExecutor === name,
    }),
    [savingExecutor],
  );

  return { savingExecutor, saveExecutorField, inlineFieldSave };
}
