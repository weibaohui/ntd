// 专家贡献 API：OAuth 登录态、预览、提交 Issue。
// 对应后端 /api/v1/contribution/* 接口。

import { api, unwrap } from './client';

/** 贡献功能登录态 */
export interface ContributionAuthStatus {
  /** 功能是否启用（后端凭据是否已注入） */
  enabled: boolean;
  /** 是否已登录（本地已存在 token） */
  logged_in: boolean;
}

/** 贡献 Issue 草稿（预览用） */
export interface ContributionIssueDraft {
  /** Issue 标题 */
  title: string;
  /** Issue Markdown 正文 */
  body: string;
  /** 已打包的文件相对路径清单 */
  files: string[];
}

/** 提交 Issue 结果 */
export interface ContributionIssueResult {
  /** Issue 编号 */
  issue_number: number;
  /** Issue 网页链接 */
  issue_url: string;
  /** Issue 标题 */
  title: string;
}

/**
 * 查询登录态与功能开关。
 */
export async function getContributionAuthStatus(): Promise<ContributionAuthStatus> {
  return unwrap(await api.get('/api/v1/contribution/auth/status'));
}

/**
 * 获取 GitCode OAuth 授权跳转 URL。
 */
export async function getContributionOAuthUrl(): Promise<{ url: string }> {
  return unwrap(await api.get('/api/v1/contribution/oauth/url'));
}

/**
 * 组装某专家的贡献 Issue 草稿（不提交），供预览框展示。
 */
export async function previewExpertIssue(name: string): Promise<ContributionIssueDraft> {
  return unwrap(await api.post(`/api/v1/contribution/experts/${encodeURIComponent(name)}/preview`));
}

/**
 * 提交某专家的贡献 Issue。
 */
export async function submitExpertIssue(
  name: string,
  data: { title: string; body: string },
): Promise<ContributionIssueResult> {
  return unwrap(await api.post(`/api/v1/contribution/experts/${encodeURIComponent(name)}/submit`, data));
}

/**
 * 清除本地登录态（退出 GitCode 登录）。
 */
export async function logoutContribution(): Promise<void> {
  await api.post('/api/v1/contribution/logout');
}
