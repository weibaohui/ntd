// 内置资源同步 API
// 对应后端 /api/bundled/* 接口
// 统一管理专家、事项模板、Skills 的远程仓库同步

import { api, unwrap } from '@/utils/database/client';

/**
 * 同步策略
 */
export type SyncStrategy = 'keep_local' | 'overwrite' | 'manual';

/**
 * 子目录类型
 */
export type Subdir = 'all' | 'experts' | 'todos' | 'skills';

export interface BundledStatus {
  remote_url: string;
  branch: string;
  local_path: string;
  sync_strategy: string;
  auto_sync_enabled: boolean;
  local_exists: boolean;
  local_commit: string | null;
  remote_commit: string | null;
  needs_update: boolean | null;
  last_sync_at: string | null;
  subdir: string;
  subdir_exists: boolean;
  subdir_file_count: number;
}

export interface BundledConfig {
  url: string;
  branch: string;
  local_path: string;
  auto_sync_enabled: boolean;
  auto_sync_cron: string;
  last_sync_at: string | null;
}

export interface SyncResult {
  success: boolean;
  message: string;
  is_first_clone: boolean;
  has_updates: boolean;
  changed_files: number;
  subdir: string;
}

/**
 * 内置资源同步 API
 */
export const bundledApi = {
  /**
   * 手动触发同步
   */
  async sync(params: { subdir?: Subdir; strategy?: SyncStrategy } = {}): Promise<SyncResult> {
    // 后端返回 {code, data, message} 包裹，必须用 unwrap 取出 data，
    // 否则调用方拿到的会是整个 axios response，字段访问全部失效。
    return unwrap(await api.post('/api/bundled/sync', {
      subdir: params.subdir || 'all',
      strategy: params.strategy || 'keep_local',
    }));
  },

  /**
   * 查询同步状态
   */
  async getStatus(subdir: Subdir = 'all'): Promise<BundledStatus> {
    return unwrap(await api.get('/api/bundled/status', { params: { subdir } }));
  },

  /**
   * 获取配置
   */
  async getConfig(): Promise<BundledConfig> {
    return unwrap(await api.get('/api/bundled/config'));
  },

  /**
   * 更新配置
   */
  async updateConfig(config: Partial<BundledConfig>): Promise<BundledConfig> {
    return unwrap(await api.put('/api/bundled/config', config));
  },
};

export default bundledApi;
