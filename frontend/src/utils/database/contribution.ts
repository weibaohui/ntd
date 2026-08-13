// 专家贡献 API：PAT 配置（保存/查询/清除）。
// 对应后端 /api/v1/contribution/* 接口。
// 提交动作由 ActionButton + 提示词驱动，不经过后端接口。

import { api, unwrap } from './client';

/** 贡献功能配置态 */
export interface ContributionAuthStatus {
  /** 是否已配置 PAT */
  configured: boolean;
}

/**
 * 查询 PAT 配置态。
 */
export async function getContributionAuthStatus(): Promise<ContributionAuthStatus> {
  return unwrap(await api.get('/api/v1/contribution/auth/status'));
}

/**
 * 保存并验证 GitCode PAT（后端调 /user 验证有效性，不存 username）。
 */
export async function saveContributionPat(pat: string): Promise<void> {
  await api.post('/api/v1/contribution/pat', { pat });
}

/**
 * 清除本地 PAT（退出 GitCode 配置）。
 */
export async function logoutContribution(): Promise<void> {
  await api.post('/api/v1/contribution/logout');
}
