import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, act, waitFor } from '@testing-library/react';
import { message } from 'antd';
import {
  useBackupDomain,
  type BackupDomainConfig,
  type BackupDomainStatusBase,
} from './useBackupDomain';

// 只用到 antd 的 message 静态方法；mock 掉避免 jsdom 中渲染通知组件的噪音
vi.mock('antd', () => ({
  message: { success: vi.fn(), error: vi.fn() },
}));

// 挂载 getStatus 失败时 hook 会 console.warn 排查；测试里静音避免输出噪音
vi.spyOn(console, 'warn').mockImplementation(() => {});

/** 构造一个最小合法状态对象——字段值刻意异于默认值，便于断言「回填自服务端」 */
function makeStatus(overrides: Partial<BackupDomainStatusBase> = {}): BackupDomainStatusBase {
  return {
    auto_backup_enabled: true,
    auto_backup_cron: '0 0 2 * * *',
    auto_backup_max_files: 7,
    last_backup: '2026-08-11T03:00:00Z',
    files: [{ name: 'a.zip', size: 1, created_at: '2026-08-11T03:00:00Z' }],
    ...overrides,
  };
}

/** 构造全 mock 的域配置；各端点默认即刻成功，测试可按需覆写 */
function makeConfig(
  overrides: Partial<BackupDomainConfig<BackupDomainStatusBase>> = {},
): BackupDomainConfig<BackupDomainStatusBase> {
  return {
    getStatus: vi.fn().mockResolvedValue(makeStatus()),
    updateAuto: vi.fn().mockResolvedValue('ok'),
    trigger: vi.fn().mockResolvedValue('手动备份完成'),
    deleteFile: vi.fn().mockResolvedValue('ok'),
    saveSuccessText: '测试域自动备份配置已保存',
    defaultCron: '0 0 9 * * *',
    ...overrides,
  };
}

beforeEach(() => {
  vi.clearAllMocks();
});

describe('useBackupDomain 初始加载', () => {
  it('test_useBackupDomain_挂载回填表单与状态', async () => {
    const config = makeConfig();
    const { result } = renderHook(() => useBackupDomain(config));

    // 回填前展示默认值（不阻塞首帧）
    expect(result.current.status).toBeNull();
    await waitFor(() => expect(result.current.status).not.toBeNull());

    expect(config.getStatus).toHaveBeenCalledTimes(1);
    expect(result.current.enabled).toBe(true);
    expect(result.current.cron).toBe('0 0 2 * * *');
    expect(result.current.maxFiles).toBe(7);
  });

  it('test_useBackupDomain_初始加载失败静默保持默认值', async () => {
    const config = makeConfig({ getStatus: vi.fn().mockRejectedValue(new Error('网络错误')) });
    const { result } = renderHook(() => useBackupDomain(config));

    // 给 rejected promise 一个 flush 窗口；随后断言状态仍为默认且无用户可见错误提示
    await act(async () => {});
    expect(result.current.status).toBeNull();
    expect(result.current.enabled).toBe(false);
    expect(result.current.cron).toBe('0 0 9 * * *');
    expect(result.current.maxFiles).toBe(30);
    // 静默语义：失败不弹用户提示（message.error 不触发）
    expect(message.error).not.toHaveBeenCalled();
    // 规范要求空 catch 至少记录：失败经 console.warn 留痕（钉死该修复，防回退成空 catch）
    expect(console.warn).toHaveBeenCalled();
  });
});

describe('useBackupDomain.triggerBackup', () => {
  it('test_triggerBackup_成功提示文案刷新状态并翻转loading', async () => {
    // 可控 deferred：把 trigger 卡在 pending，稳定捕获 loading=true 的中间态
    let resolveTrigger!: (v: string) => void;
    const config = makeConfig({
      trigger: vi.fn().mockImplementation(() => new Promise<string>((r) => (resolveTrigger = r))),
    });
    const { result } = renderHook(() => useBackupDomain(config));
    await waitFor(() => expect(result.current.status).not.toBeNull());

    act(() => {
      void result.current.triggerBackup();
    });
    expect(result.current.loading).toBe(true);

    await act(async () => {
      resolveTrigger('手动备份完成');
    });
    expect(result.current.loading).toBe(false);
    expect(message.success).toHaveBeenCalledWith('手动备份完成');
    // 初始 1 次 + trigger 成功后刷新 1 次
    expect(config.getStatus).toHaveBeenCalledTimes(2);
  });

  it('test_triggerBackup_失败透传后端错误文案', async () => {
    const config = makeConfig({ trigger: vi.fn().mockRejectedValue(new Error('磁盘已满')) });
    const { result } = renderHook(() => useBackupDomain(config));
    await waitFor(() => expect(result.current.status).not.toBeNull());

    await act(async () => {
      await result.current.triggerBackup();
    });
    expect(message.error).toHaveBeenCalledWith('磁盘已满');
    expect(result.current.loading).toBe(false);
    // 失败不刷新状态
    expect(config.getStatus).toHaveBeenCalledTimes(1);
  });

  it('test_triggerBackup_无后端文案时用兜底', async () => {
    const config = makeConfig({ trigger: vi.fn().mockRejectedValue({}) });
    const { result } = renderHook(() => useBackupDomain(config));
    await waitFor(() => expect(result.current.status).not.toBeNull());

    await act(async () => {
      await result.current.triggerBackup();
    });
    // 非 Error、无 message 的对象 → 回落操作级兜底文案（extractErrorMessage 的非对象/无 message 分支）
    expect(message.error).toHaveBeenCalledWith('备份失败');
  });
});

describe('useBackupDomain.saveAutoBackup', () => {
  it('test_saveAutoBackup_以表单值保存且不刷新状态', async () => {
    const config = makeConfig();
    const { result } = renderHook(() => useBackupDomain(config));
    await waitFor(() => expect(result.current.status).not.toBeNull());

    // 用户在界面上改了表单再保存
    act(() => {
      result.current.setEnabled(false);
      result.current.setCron('0 0 6 * * *');
      result.current.setMaxFiles(15);
    });
    await act(async () => {
      await result.current.saveAutoBackup();
    });

    expect(config.updateAuto).toHaveBeenCalledWith(false, '0 0 6 * * *', 15);
    expect(message.success).toHaveBeenCalledWith('测试域自动备份配置已保存');
    expect(config.getStatus).toHaveBeenCalledTimes(1);
  });
});

describe('useBackupDomain.deleteBackup', () => {
  it('test_deleteBackup_删除不翻转loading且成功刷新', async () => {
    // 用 deferred 把 deleteFile 卡在 pending，稳定捕获「删除进行中」的 loading 态——
    // 钉死原语义：删除不点亮 loading 灯（若误走 runWithLoading，pending 时 loading 会变 true 而回归）。
    let resolveDelete!: (v: unknown) => void;
    const config = makeConfig({
      deleteFile: vi.fn().mockImplementation(() => new Promise((r) => (resolveDelete = r))),
    });
    const { result } = renderHook(() => useBackupDomain(config));
    await waitFor(() => expect(result.current.status).not.toBeNull());

    act(() => {
      void result.current.deleteBackup('a.zip');
    });
    // 删除进行中：loading 必须保持 false（原 delete handler 刻意不翻转 loading）
    expect(result.current.loading).toBe(false);

    await act(async () => {
      resolveDelete('ok');
    });
    expect(config.deleteFile).toHaveBeenCalledWith('a.zip');
    expect(message.success).toHaveBeenCalledWith('已删除');
    expect(result.current.loading).toBe(false);
    // 初始 1 次 + 删除成功后刷新 1 次
    expect(config.getStatus).toHaveBeenCalledTimes(2);
  });

  it('test_deleteBackup_失败透传错误且不刷新', async () => {
    const config = makeConfig({ deleteFile: vi.fn().mockRejectedValue(new Error('文件被占用')) });
    const { result } = renderHook(() => useBackupDomain(config));
    await waitFor(() => expect(result.current.status).not.toBeNull());

    await act(async () => {
      await result.current.deleteBackup('a.zip');
    });
    expect(message.error).toHaveBeenCalledWith('文件被占用');
    expect(config.getStatus).toHaveBeenCalledTimes(1);
  });
});

describe('useBackupDomain.runWithLoading', () => {
  it('test_runWithLoading_附加操作复用loading并按文案报错', async () => {
    const config = makeConfig();
    const { result } = renderHook(() => useBackupDomain(config));
    await waitFor(() => expect(result.current.status).not.toBeNull());

    const ok = vi.fn().mockResolvedValue(undefined);
    await act(async () => {
      await result.current.runWithLoading(ok, '优化失败');
    });
    expect(ok).toHaveBeenCalledTimes(1);
    expect(message.error).not.toHaveBeenCalled();

    await act(async () => {
      await result.current.runWithLoading(async () => {
        throw new Error('vacuum 中断');
      }, '优化失败');
    });
    expect(message.error).toHaveBeenCalledWith('vacuum 中断');
    expect(result.current.loading).toBe(false);
  });
});
