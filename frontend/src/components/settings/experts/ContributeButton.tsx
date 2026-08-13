// 专家「分享到官方仓库」按钮：OAuth 登录引导 + 预览确认 + 提交 Issue。
// 独立「专家」页与「专家模板」Tab 复用，保证两条入口交互一致。

import { useState } from 'react';
import { App, Button, Input, Modal, Space, Tag, Tooltip } from 'antd';
import { ShareAltOutlined } from '@ant-design/icons';
import type { ExpertMetadata } from '@/types/expert';
import {
  getContributionAuthStatus,
  getContributionOAuthUrl,
  previewExpertIssue,
  submitExpertIssue,
  type ContributionIssueDraft,
} from '@/utils/database/contribution';

/**
 * 分享按钮组件。
 *
 * 交互流程：
 * 1. 点击后查询登录态：未配置凭据 → 提示；未登录 → 跳 GitCode 授权页；已登录 → 拉预览草稿。
 * 2. 弹出预览 Modal：标题/正文可编辑，文件清单只读；确认后提交并展示 Issue 链接。
 */
export function ContributeButton({
  expert,
  size = 'middle',
}: {
  expert: ExpertMetadata;
  size?: 'small' | 'middle';
}) {
  const { message } = App.useApp();
  const [loading, setLoading] = useState(false);
  const [draft, setDraft] = useState<ContributionIssueDraft | null>(null);
  const [title, setTitle] = useState('');
  const [body, setBody] = useState('');
  const [submitting, setSubmitting] = useState(false);

  // 点击分享：未配置提示、未登录跳授权、已登录拉预览。
  const handleClick = async () => {
    setLoading(true);
    try {
      const status = await getContributionAuthStatus();
      if (!status.enabled) {
        message.warning('贡献功能未配置（缺少 OAuth 凭据）');
        return;
      }
      if (!status.logged_in) {
        const { url } = await getContributionOAuthUrl();
        // 跳 GitCode 授权页；授权成功后后端 302 回专家页，用户再点一次分享即可进入预览。
        window.location.href = url;
        return;
      }
      const d = await previewExpertIssue(expert.name);
      setDraft(d);
      setTitle(d.title);
      setBody(d.body);
    } catch (err: any) {
      message.error('发起分享失败: ' + (err?.message || String(err)));
    } finally {
      setLoading(false);
    }
  };

  // 确认提交 Issue。
  const handleSubmit = async () => {
    setSubmitting(true);
    try {
      const result = await submitExpertIssue(expert.name, { title, body });
      setDraft(null);
      message.success({
        content: (
          <span>
            已提交为 Issue #{result.issue_number}，{' '}
            <a href={result.issue_url} target="_blank" rel="noreferrer">点击查看</a>
          </span>
        ),
        duration: 10,
      });
    } catch (err: any) {
      message.error('提交失败: ' + (err?.message || String(err)));
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <>
      <Tooltip title="分享到官方仓库">
        <Button
          type="text"
          size={size}
          icon={<ShareAltOutlined />}
          onClick={handleClick}
          loading={loading}
          style={{ color: 'var(--color-text-secondary)' }}
        >
          分享
        </Button>
      </Tooltip>

      <Modal
        open={draft !== null}
        onCancel={() => setDraft(null)}
        onOk={handleSubmit}
        okText="提交"
        cancelText="取消"
        confirmLoading={submitting}
        title="提交到官方仓库"
        width={680}
        centered
      >
        {draft && (
          <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
            <div>
              <div style={{ fontSize: 12, color: 'var(--color-text-secondary)', marginBottom: 4 }}>
                Issue 标题
              </div>
              <Input value={title} onChange={(e) => setTitle(e.target.value)} />
            </div>
            <div>
              <div style={{ fontSize: 12, color: 'var(--color-text-secondary)', marginBottom: 4 }}>
                将打包以下文件（{draft.files.length} 个）
              </div>
              <Space wrap size={4}>
                {draft.files.map((f) => (
                  <Tag key={f} style={{ fontSize: 11 }}>{f}</Tag>
                ))}
              </Space>
            </div>
            <div>
              <div style={{ fontSize: 12, color: 'var(--color-text-secondary)', marginBottom: 4 }}>
                Issue 正文（可编辑）
              </div>
              <Input.TextArea
                value={body}
                onChange={(e) => setBody(e.target.value)}
                autoSize={{ minRows: 8, maxRows: 20 }}
                style={{ fontSize: 12, fontFamily: 'monospace' }}
              />
            </div>
            <div style={{ fontSize: 12, color: 'var(--color-text-secondary)' }}>
              提交后由官方维护者审核，合入后其他用户可通过同步获取。
            </div>
          </div>
        )}
      </Modal>
    </>
  );
}
