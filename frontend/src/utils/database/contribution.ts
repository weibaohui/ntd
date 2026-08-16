// 专家贡献 API：PAT 配置（保存/查询/清除）。
// 对应后端 /api/v1/contribution/* 接口。
// 提交动作由 ActionButton + 提示词驱动，不经过后端接口。

import { api, unwrap } from './client';

/** 贡献功能配置态 */
export interface ContributionAuthStatus {
  /** 是否已配置 PAT */
  configured: boolean;
}

/** PAT 验证结果：PAT 所属账号的用户名（证明令牌当前可用）。 */
export interface ContributionVerifyResult {
  /** 账号登录名（GitCode /user 的 login 字段） */
  username: string;
  /** 显示名；为空时前端展示回退 username */
  name: string;
}

/**
 * 查询 PAT 配置态。
 */
export async function getContributionAuthStatus(): Promise<ContributionAuthStatus> {
  return unwrap(await api.get('/api/v1/contribution/auth/status'));
}

/**
 * 验证已保存的 PAT 并获取所属账号用户名（设置页「验证」按钮调用）。
 * 后端读取本地 PAT 调 GitCode /user；未配置/无效/网络故障均抛错，由调用方展示。
 */
export async function verifyContributionPat(): Promise<ContributionVerifyResult> {
  return unwrap(await api.get('/api/v1/contribution/verify'));
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
