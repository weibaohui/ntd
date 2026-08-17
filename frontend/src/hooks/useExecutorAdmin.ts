/**
 * useExecutorAdmin — 执行器列表管理数据族 hook（096-W4-4-3 产物）。
 *
 * 承接原 ExecutorsPanel 主组件的「执行器列表 + 检测/测试/修复/设默认/安装刷新 + 模型缓存」整块职责：
 * - 列表族（executors / executorsLoading）与 loader（loadExecutors）
 * - 检测族（detectResults / detectingExecutor / batchDetecting）+ 单行检测 / 批量检测 / 清除检测结果
 * - 测试族（testingExecutor / testModalVisible / testModalData）+ 单行测试
 * - 设为默认（settingDefaultExecutor pending-by-name）+ 修复 + 安装后刷新
 * - 默认模型列的模型列表懒加载缓存（executorModels / modelsLoading）
 *
 * 设计取舍：
 * - 4 个 pending-by-name state（detectingExecutor / testingExecutor / settingDefaultExecutor 等
 *   `string | null`）保留原语义（同名互斥 loading，避免同一执行器并发操作），仅内聚进 hook，
 *   不强行 useReducer 化——行为零风险优先（见 doc 101 风险节）。
 * - replaceExecutor 是「按 name 替换列表项」的唯一入口，供本 hook 各 handler 与
 *   行内保存 hook（useExecutorFieldSaver）共用，避免两处各自 map-replace 漂移。
 *
 * 函数体逐字搬自主组件原实现，行为等价。
 */

import { useCallback, useEffect, useRef, useState } from 'react';
import { App } from 'antd';
import type { ExecutorConfig } from '@/types';
import * as db from '@/utils/database';
import { setDefaultExecutorCache } from '@/utils/executors';

/** 单次检测的结果（binary 是否找到 + 解析后的绝对路径）。 */
interface DetectResult {
  found: boolean;
  resolved: string | null;
}

/** 单次测试的结果（通过与否 + 输出/错误）。Modal 据此渲染。 */
interface ExecutorTestResult {
  test_passed: boolean;
  output: string | null;
  error: string | null;
}

/** 测试 Modal 的展示数据：被测执行器名 + 结果体。 */
interface ExecutorTestModalData {
  name: string;
  result: ExecutorTestResult;
}

export interface UseExecutorAdminReturn {
  // ── 列表族 ──
  executors: ExecutorConfig[];
  executorsLoading: boolean;
  /** 首屏加载执行器配置列表（主组件 useEffect 调一次）。 */
  loadExecutors: () => Promise<void>;
  /** 按 name 替换列表中对应项（增/改后刷新前端缓存的唯一入口）。 */
  replaceExecutor: (name: string, updated: ExecutorConfig) => void;

  // ── 检测族 ──
  detectResults: Record<string, DetectResult>;
  detectingExecutor: string | null;
  /** 单行「检测」按钮：探测二进制可用性并落检测结果。 */
  detectExecutorByName: (ec: ExecutorConfig) => Promise<void>;
  /** 清除某执行器的检测结果（改路径后让旧结果失效，强制重新检测）。 */
  clearDetectResult: (name: string) => void;
  batchDetecting: boolean;
  /** 「批量检测」按钮：遍历全部执行器，按检测结果自动翻转 enabled。 */
  batchDetect: () => Promise<void>;

  // ── 测试族 ──
  testingExecutor: string | null;
  /** 单行「测试」按钮：跑一次执行器自检，弹 Modal 展示结果。 */
  testExecutorByName: (ec: ExecutorConfig) => Promise<void>;
  testModalVisible: boolean;
  testModalData: ExecutorTestModalData | null;
  closeTestModal: () => void;

  // ── 设为默认 / 修复 / 安装后刷新 ──
  settingDefaultExecutor: string | null;
  /** 「设为默认」按钮：置该执行器为系统默认，翻转全表 is_default 标记。 */
  setAsDefault: (ec: ExecutorConfig) => Promise<void>;
  /** 「修复」按钮（仅检测不可用时出现）：尝试定位并回写正确路径。 */
  repairByName: (ec: ExecutorConfig) => Promise<void>;
  /** 安装执行器后的刷新：detect→repair→updateExecutor 三步兜底链。 */
  refreshAfterInstall: (ec: ExecutorConfig) => Promise<void>;

  // ── 默认模型列的模型列表懒加载缓存 ──
  executorModels: Record<string, string[]>;
  modelsLoading: Record<string, boolean>;
  /** Select 下拉展开时按需拉取该执行器支持的模型（按 name 缓存，空结果也缓存）。 */
  handleModelsDropdown: (name: string, open: boolean) => void;
}

export function useExecutorAdmin(): UseExecutorAdminReturn {
  const { message } = App.useApp();

  // ── 列表族 ──
  const [executors, setExecutors] = useState<ExecutorConfig[]>([]);
  const [executorsLoading, setExecutorsLoading] = useState(false);

  // ── 检测族 ──
  // detectResults 按 name 缓存每行最近一次检测结果，供「检测状态」列与操作列条件渲染。
  const [detectResults, setDetectResults] = useState<Record<string, DetectResult>>({});
  // detectingExecutor / testingExecutor / settingDefaultExecutor 三个 pending-by-name：
  // 同名互斥 loading——同一执行器同时只能有一个进行中操作，避免并发相互覆盖前端缓存。
  const [detectingExecutor, setDetectingExecutor] = useState<string | null>(null);
  const [testingExecutor, setTestingExecutor] = useState<string | null>(null);
  const [batchDetecting, setBatchDetecting] = useState(false);
  const [settingDefaultExecutor, setSettingDefaultExecutor] = useState<string | null>(null);

  // ── 测试 Modal 族 ──
  const [testModalVisible, setTestModalVisible] = useState(false);
  const [testModalData, setTestModalData] = useState<ExecutorTestModalData | null>(null);

  // ── 默认模型列的模型列表懒加载缓存 ──
  // 各执行器可选模型（调 models 子命令拉取），用于默认模型列下拉建议，按 name 缓存。
  // 空数组（[]）也缓存，避免 supports_models 执行器每次展开都请求——空结果意味着该执行器不支持动态列举。
  const [executorModels, setExecutorModels] = useState<Record<string, string[]>>({});
  // 各执行器模型加载状态：true = 请求进行中，false/undefined = 空闲。
  // 与 executorModels 分开维护，避免空结果被误判为「正在加载」。
  const [modelsLoading, setModelsLoading] = useState<Record<string, boolean>>({});
  // 记录已拉取过的执行器（ref 是同步的，在异步请求前就标记，避免并发重复请求）。
  const fetchedModelsRef = useRef<Record<string, boolean>>({});

  /** 首屏加载执行器配置列表。 */
  const loadExecutors = useCallback(async () => {
    try {
      setExecutorsLoading(true);
      const list = await db.getExecutors();
      setExecutors(list);
    } catch (err: unknown) {
      // 前端 API 已把后端错误统一包成 Error（见 client.ts 的 unwrap），可安全读 message。
      message.error('加载执行器配置失败: ' + (err instanceof Error ? err.message : String(err)));
    } finally {
      setExecutorsLoading(false);
    }
  }, [message]);

  /** 按 name 替换列表中对应项；不存在的 name 不变（保守，不新增）。 */
  const replaceExecutor = useCallback((name: string, updated: ExecutorConfig) => {
    setExecutors((prev) => prev.map((e) => (e.name === name ? updated : e)));
  }, []);

  /** 单行「检测」按钮：探测二进制可用性，落检测结果并提示。 */
  const detectExecutorByName = useCallback(
    async (ec: ExecutorConfig) => {
      setDetectingExecutor(ec.name);
      try {
        const result = await db.detectExecutor(ec.name);
        // 落检测结果：found 决定「检测状态」列绿/红，resolved 供 Tooltip 展示绝对路径。
        setDetectResults((prev) => ({
          ...prev,
          [ec.name]: { found: result.binary_found, resolved: result.path_resolved },
        }));
        if (result.binary_found) {
          message.success(`${ec.display_name}: 找到 (${result.path_resolved})`);
        } else {
          message.warning(`${ec.display_name}: 未找到`);
        }
      } catch (err: unknown) {
        message.error('检测失败: ' + (err instanceof Error ? err.message : String(err)));
      } finally {
        setDetectingExecutor(null);
      }
    },
    [message],
  );

  /** 清除某执行器的检测结果（改路径后让旧结果失效）。 */
  const clearDetectResult = useCallback((name: string) => {
    setDetectResults((prev) => {
      const next = { ...prev };
      delete next[name];
      return next;
    });
  }, []);

  /**
   * 「批量检测」按钮：遍历全部执行器，按检测结果自动翻转 enabled。
   *
   * 找到二进制但未启用 → 自动启用；未找到但已启用 → 自动禁用。
   * 单个执行器检测失败不中断整体（只 warn），保证批量能跑完。
   */
  const batchDetect = useCallback(async () => {
    setBatchDetecting(true);
    // 本轮可用计数，结束后汇总提示「N/M 可用」。
    let availableCount = 0;
    try {
      for (const ec of executors) {
        try {
          const result = await db.detectExecutor(ec.name);
          setDetectResults((prev) => ({
            ...prev,
            [ec.name]: { found: result.binary_found, resolved: result.path_resolved },
          }));
          if (result.binary_found) {
            availableCount++;
            // 找到但未启用：自动启用，刷新列表项。
            if (!ec.enabled) {
              const updated = await db.updateExecutor(ec.name, { enabled: true });
              replaceExecutor(ec.name, updated);
            }
          } else if (ec.enabled) {
            // 未找到但已启用：自动禁用，刷新列表项。
            const updated = await db.updateExecutor(ec.name, { enabled: false });
            replaceExecutor(ec.name, updated);
          }
        } catch (err) {
          // 单个执行器检测失败不中断批量；记录执行器名便于排查（原实现静默吞，此处补 warn 满足禁止清单 #6）。
          console.warn(`批量检测 ${ec.name} 失败:`, err);
        }
      }
      message.success(`批量检测完成：${availableCount}/${executors.length} 个执行器可用`);
    } catch (err: unknown) {
      message.error('批量检测失败: ' + (err instanceof Error ? err.message : String(err)));
    } finally {
      setBatchDetecting(false);
    }
  }, [executors, message, replaceExecutor]);

  /** 单行「测试」按钮：跑一次执行器自检，弹 Modal 展示结果。 */
  const testExecutorByName = useCallback(
    async (ec: ExecutorConfig) => {
      setTestingExecutor(ec.name);
      try {
        const result = await db.testExecutor(ec.name);
        setTestModalData({ name: ec.name, result });
        setTestModalVisible(true);
      } catch (err: unknown) {
        message.error('测试失败: ' + (err instanceof Error ? err.message : String(err)));
      } finally {
        setTestingExecutor(null);
      }
    },
    [message],
  );

  /** 关闭测试结果 Modal（保留 testModalData 供下次复用渲染，仅切显隐）。 */
  const closeTestModal = useCallback(() => {
    setTestModalVisible(false);
  }, []);

  /** 「设为默认」按钮：置该执行器为系统默认，翻转全表 is_default 标记。 */
  const setAsDefault = useCallback(
    async (ec: ExecutorConfig) => {
      // 已是默认则无操作（按钮本应 disabled，双保险）。
      if (ec.is_default) return;
      setSettingDefaultExecutor(ec.name);
      try {
        const updated = await db.setDefaultExecutor(ec.name);
        // 更新前端缓存，使新的默认值立即生效（执行入口读缓存而非每次查库）。
        setDefaultExecutorCache(updated.name);
        // 后端只返回新默认执行器，前端需自行把全表 is_default 重算：仅新默认为 true，其余置 false。
        setExecutors((prev) =>
          prev.map((e) => ({
            ...e,
            is_default: e.name === updated.name,
          })),
        );
        message.success(`${ec.display_name} 已设为默认执行器`);
      } catch (err: unknown) {
        message.error('设置失败: ' + (err instanceof Error ? err.message : String(err)));
      } finally {
        setSettingDefaultExecutor(null);
      }
    },
    [message],
  );

  /** 「修复」按钮（仅检测不可用时出现）：尝试定位并回写正确路径。 */
  const repairByName = useCallback(
    async (ec: ExecutorConfig) => {
      try {
        const result = await db.repairExecutor(ec.name);
        if (result.binary_found) {
          // 修复成功：落检测结果并回写新路径 + 强制启用。
          // path_resolved 非空由 binary_found=true 保证，用 `!` 断言简化（后端契约）。
          setDetectResults((prev) => ({
            ...prev,
            [ec.name]: { found: true, resolved: result.path_resolved! },
          }));
          const updated = await db.updateExecutor(ec.name, {
            path: result.path_resolved!,
            enabled: true,
          });
          replaceExecutor(ec.name, updated);
          // path_updated 区分「真改了路径」与「路径本就对」两种提示语义。
          if (result.path_updated) {
            message.success(`已修复：${ec.display_name} 路径更新为 ${result.path_resolved}`);
          } else {
            message.info(`路径已是最新：${result.path_resolved}`);
          }
        } else {
          message.error(`未找到 ${ec.display_name}，请手动填写路径`);
        }
      } catch (err: unknown) {
        message.error('修复失败: ' + (err instanceof Error ? err.message : String(err)));
      }
    },
    [message, replaceExecutor],
  );

  /**
   * 安装执行器后的刷新：detect→repair→updateExecutor 三步兜底链。
   *
   * 安装完成回调里重新检测；若找到则修复路径 + 强制启用 + 刷新前端状态。
   * 路径优先级：repair 更新路径 > detect 检测路径 > 数据库原有路径，
   * 因为 repair 后端可能返回更新后的路径，detect 仅返回 which 结果，record.path 是旧值。
   * 强制 enabled:true 是因为 detect 成功说明 binary 可用，没必要让用户手动去开开关。
   */
  const refreshAfterInstall = useCallback(
    async (ec: ExecutorConfig) => {
      try {
        const detect = await db.detectExecutor(ec.name);
        setDetectResults((prev) => ({
          ...prev,
          [ec.name]: { found: detect.binary_found, resolved: detect.path_resolved },
        }));
        if (detect.binary_found) {
          const repair = await db.repairExecutor(ec.name);
          // 三选一兜底：repair 可能更新路径，detect 仅 which 结果，ec.path 是旧值。
          const updatedPath = repair.path_resolved || detect.path_resolved || ec.path;
          const updated = await db.updateExecutor(ec.name, {
            path: updatedPath,
            enabled: true,
          });
          replaceExecutor(ec.name, updated);
          message.success(`${ec.display_name} 安装/修复完成：${updatedPath}`);
        } else {
          // 安装后仍检测不到：可能 PATH 未刷新、需要新开终端、或安装脚本失败。
          message.warning(`${ec.display_name} 安装后仍未检测到，请检查安装日志或手动填写路径`);
        }
      } catch (err: unknown) {
        message.error('刷新执行器状态失败: ' + (err instanceof Error ? err.message : String(err)));
      }
    },
    [message, replaceExecutor],
  );

  /**
   * 拉取某执行器支持的模型列表（按 name 缓存，空结果也缓存）。
   *
   * 空名或已请求过（含空结果）→ 跳过；在 await 前标记已请求避免并发重入。
   * 请求失败也写空数组，避免「一直加载中、出不来结果」的 stuck 状态。
   */
  const fetchExecutorModels = useCallback(async (name: string) => {
    if (!name || fetchedModelsRef.current[name]) return;
    // 在 await 前标记已请求，避免并发重入。
    fetchedModelsRef.current[name] = true;
    setModelsLoading((prev) => ({ ...prev, [name]: true }));
    try {
      const models = await db.getExecutorModels(name);
      // 无论结果是否为空都缓存，让 fetchedModelsRef 拦截后续请求。
      setExecutorModels((prev) => ({ ...prev, [name]: models }));
    } catch (err: unknown) {
      // 请求失败也写空数组，避免「一直加载中」的 stuck 状态；记录原因便于排查（禁止清单 #6：空 catch 需留痕）。
      console.warn(`拉取执行器 ${name} 模型列表失败`, err);
      setExecutorModels((prev) => ({ ...prev, [name]: [] }));
    } finally {
      setModelsLoading((prev) => ({ ...prev, [name]: false }));
    }
  }, []);

  /** Select 下拉展开时按需拉取模型（收起时不动作）。 */
  const handleModelsDropdown = useCallback(
    (name: string, open: boolean) => {
      // 仅展开且尚未请求过时触发；ref 的置位交给 fetchExecutorModels 统一管理（单一事实源）。
      // 不在此处预置 ref——原实现在此预置后立刻调 fetchExecutorModels，后者自身的 ref 守卫随即命中
      // 早退，导致下拉展开永远拉不到模型（自废武功的死代码）。顺手修正：ref 只在 fetcher 内一处置位。
      if (open && !fetchedModelsRef.current[name]) {
        fetchExecutorModels(name);
      }
    },
    [fetchExecutorModels],
  );

  // 首屏自动加载执行器列表（延续原主组件 useEffect 的 mount 加载语义）。
  useEffect(() => {
    loadExecutors();
  }, [loadExecutors]);

  return {
    executors,
    executorsLoading,
    loadExecutors,
    replaceExecutor,
    detectResults,
    detectingExecutor,
    detectExecutorByName,
    clearDetectResult,
    batchDetecting,
    batchDetect,
    testingExecutor,
    testExecutorByName,
    testModalVisible,
    testModalData,
    closeTestModal,
    settingDefaultExecutor,
    setAsDefault,
    repairByName,
    refreshAfterInstall,
    executorModels,
    modelsLoading,
    handleModelsDropdown,
  };
}
