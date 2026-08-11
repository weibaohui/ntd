import { useCallback, useEffect, useRef, useState } from 'react';
import { message } from 'antd';

/**
 * 单备份域的状态与操作收口 hook（096-W1-PR4 产物）。
 *
 * 原 BackupPanel 为 database/todo/skill 三个域各维护一组逐字同构的状态与 handler：
 * 5 个 useState ×3 + 4 个 handler ×3 + 初始加载 useEffect ×3（约 150 行重复），
 * 唯一差异只是「调用的 4 个后端端点 + 保存成功文案 + 默认 cron」。
 * 本 hook 把同构部分收敛为一份，域差异经 config 参数注入。
 */

/** 三域备份状态的公共基底（Skill 域多出的 executor_skills 字段经泛型 S 保留）。 */
export interface BackupDomainStatusBase {
  auto_backup_enabled: boolean;
  auto_backup_cron: string;
  auto_backup_max_files: number;
  last_backup: string | null;
  files: { name: string; size: number; created_at: string }[];
}

/** 单个备份域的差异面：4 个后端端点 + 2 处文案/默认值。 */
export interface BackupDomainConfig<S extends BackupDomainStatusBase> {
  /** 拉取该域备份状态（挂载时调用一次，trigger/delete 成功后也会刷新） */
  getStatus: () => Promise<S>;
  /** 保存自动备份配置（enabled/cron/maxFiles 三元组） */
  updateAuto: (enabled: boolean, cron: string, maxFiles?: number) => Promise<unknown>;
  /** 触发一次手动备份，返回后端提示文案 */
  trigger: () => Promise<string>;
  /** 删除指定备份文件 */
  deleteFile: (filename: string) => Promise<unknown>;
  /** 保存成功提示（各域带名前缀，如 'Todo自动备份配置已保存'） */
  saveSuccessText: string;
  /** 初始 cron 兜底值（各域错开备份时刻：3/4/5 点），getStatus 回填前的展示值 */
  defaultCron: string;
}

/** hook 返回面：状态 + 表单 setter + 三个域操作 + 通用 loading 包装。 */
export interface BackupDomain<S extends BackupDomainStatusBase> {
  status: S | null;
  enabled: boolean;
  cron: string;
  maxFiles: number;
  loading: boolean;
  setEnabled: (v: boolean) => void;
  setCron: (v: string) => void;
  setMaxFiles: (v: number) => void;
  /** 手动触发备份：成功提示后端文案并刷新状态 */
  triggerBackup: () => Promise<void>;
  /** 以当前表单值保存自动备份配置 */
  saveAutoBackup: () => Promise<void>;
  /** 删除备份文件并刷新状态 */
  deleteBackup: (filename: string) => Promise<void>;
  /**
   * 域内附加操作（如数据库优化/日志清理）的 loading 包装：
   * 与域操作共享同一 loading 指示灯——与原实现中这些操作复用 setBackupLoading 的行为一致。
   */
  runWithLoading: (action: () => Promise<void>, errorText: string) => Promise<void>;
}

export function useBackupDomain<S extends BackupDomainStatusBase>(
  config: BackupDomainConfig<S>,
): BackupDomain<S> {
  const [status, setStatus] = useState<S | null>(null);
  const [enabled, setEnabled] = useState(false);
  const [cron, setCron] = useState(config.defaultCron);
  const [maxFiles, setMaxFiles] = useState(30);
  const [loading, setLoading] = useState(false);

  // latest-ref 模式：调用方每次渲染传入新的 config 字面量，但端点函数本身是模块级稳定引用。
  // 用 ref 持有后，初始加载可安全地只跑一次（与原实现三个 useEffect(..., []) 行为逐字一致），
  // 不因 config 字面量身份变化而重复拉取。
  const configRef = useRef(config);
  configRef.current = config;

  // 挂载时拉一次状态并回填表单（原三份逐字同构的初始加载 useEffect 收口于此）。
  // 拉取失败静默保持默认值——面板仅展示态受影响，不阻断用户操作，与原实现 .catch(() => {}) 一致。
  useEffect(() => {
    configRef.current
      .getStatus()
      .then((s) => {
        setStatus(s);
        setEnabled(s.auto_backup_enabled);
        setCron(s.auto_backup_cron);
        setMaxFiles(s.auto_backup_max_files);
      })
      .catch(() => {});
  }, []);

  // 通用 loading 包装：setLoading 翻转 + 统一 catch 文案提示 + finally 复位。
  // 域内所有写操作（trigger/save/delete 及数据库域的优化/日志清理）共用这一条骨架。
  const runWithLoading = useCallback(
    async (action: () => Promise<void>, errorText: string) => {
      setLoading(true);
      try {
        await action();
      } catch (err: any) {
        // 后端有具体错误信息则透传，否则用操作级兜底文案——与原实现的 err?.message || '...' 一致
        message.error(err?.message || errorText);
      } finally {
        setLoading(false);
      }
    },
    [],
  );

  // 手动备份：成功顺序为「提示后端文案 → 重新拉状态」——与原 handler 逐字一致
  const triggerBackup = useCallback(
    () =>
      runWithLoading(async () => {
        message.success(await configRef.current.trigger());
        setStatus(await configRef.current.getStatus());
      }, '备份失败'),
    [runWithLoading],
  );

  // 保存自动备份配置：成功后不刷新 status——原实现即如此（表单值即真相，无需回拉）
  const saveAutoBackup = useCallback(
    () =>
      runWithLoading(async () => {
        await configRef.current.updateAuto(enabled, cron, maxFiles);
        message.success(configRef.current.saveSuccessText);
      }, '保存失败'),
    [runWithLoading, enabled, cron, maxFiles],
  );

  // 删除文件：成功后刷新状态让文件列表即时消失——与原 handler 逐字一致
  const deleteBackup = useCallback(
    (filename: string) =>
      runWithLoading(async () => {
        await configRef.current.deleteFile(filename);
        message.success('已删除');
        setStatus(await configRef.current.getStatus());
      }, '删除失败'),
    [runWithLoading],
  );

  return {
    status,
    enabled,
    cron,
    maxFiles,
    loading,
    setEnabled,
    setCron,
    setMaxFiles,
    triggerBackup,
    saveAutoBackup,
    deleteBackup,
    runWithLoading,
  };
}
