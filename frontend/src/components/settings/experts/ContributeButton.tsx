// 专家「分享到官方仓库」按钮：PAT 配置引导 + ActionButton 提示词驱动提交 PR。
// 独立「专家」页与「专家模板」Tab 复用，保证两条入口交互一致。
// PAT 的填写/保存/清空统一收口到「设置 → 第三方授权 → GitCode」，
// 分享按钮旁不放清除入口（易误触），未配置 PAT 时跳转设置引导填写。

import { useState } from 'react';
import { App, Button, Space, Tooltip, Typography } from 'antd';
import { ShareAltOutlined } from '@ant-design/icons';
import { ActionButton } from '@/components/ActionButton';
import { useTodos } from '@/hooks/useTodoContext';
import { useViewState } from '@/hooks/useViewState';
import type { ExpertMetadata } from '@/types/expert';
import { getContributionAuthStatus } from '@/utils/database/contribution';
import { THIRD_PARTY_SETTINGS_TAB } from '@/components/settings/ThirdPartyPanel';
import { CONTRIBUTE_ACTION_TYPE, buildContributePrompt } from './contributePrompt';

/**
 * 把定义目录的绝对路径转成 ~/ 相对路径，避免在 prompt/执行记录里暴露家目录下的用户名。
 * 专家目录一定在 ~/.ntd/ 下（~/.ntd/experts/ 或 ~/.ntd/bundled/experts/）。
 */
function toHomePath(absPath: string): string {
  const marker = '/.ntd/';
  const idx = absPath.indexOf(marker);
  // 找到 /.ntd/ 就把其前缀（家目录）替换为 ~；找不到则原样返回（理论上不会发生）。
  return idx >= 0 ? `~${absPath.slice(idx)}` : absPath;
}

/**
 * 分享按钮组件。
 *
 * 交互流程：
 * 1. 点击「分享」→ 查询 PAT 配置态：未配置 → 提示并跳转设置-第三方授权；已配置 → 切换为 ActionButton。
 * 2. ActionButton：打开 Drawer，展示/编辑提交 prompt，选执行器后由 AI 执行
 *    （读取本机 PAT、调用 GitCode API 完成 fork → 建分支 → 写文件 → 建 PR）。
 */
export function ContributeButton({
  expert,
  size = 'middle',
}: {
  expert: ExpertMetadata;
  size?: 'small' | 'middle';
}) {
  const { message } = App.useApp();
  const { Text, Paragraph } = Typography;
  // 当前左上角选中的工作空间：提交时默认用它，与全局选择保持一致。
  const { state } = useTodos();
  // 未配置 PAT 时跳转设置-第三方授权，用全局导航保证 URL 与左侧菜单高亮同步。
  const { showView } = useViewState();
  const [checking, setChecking] = useState(false);
  // 是否已确认 PAT 配置：true 时渲染 ActionButton 执行提交。
  const [configured, setConfigured] = useState(false);

  // 分享只对用户自定义专家开放（source === 'user'）：
  // 系统/模板来源（从官方仓库同步到 ~/.ntd/bundled/experts/）的专家是只读资源，
  // 用户不能修改，也就不能把系统专家原样打包提 PR 回官方仓库——直接不渲染分享入口。
  // 双入口（专家详情 Modal + 专家模板 Tab 行操作）复用本组件，此处守卫同时覆盖两处。
  if (expert.source !== 'user') return null;

  // 点击分享：查 PAT 配置态，已配置切到 ActionButton，未配置引导去设置填写。
  const handleClick = async () => {
    setChecking(true);
    try {
      const status = await getContributionAuthStatus();
      if (status.configured) {
        setConfigured(true);
      } else {
        // PAT 管理已收口到设置：这里只给提示并跳转，不再内嵌填写框。
        message.info('请先在「设置 → 第三方授权 → GitCode」中填写并保存 PAT');
        showView('settings', { tab: THIRD_PARTY_SETTINGS_TAB });
      }
    } catch (err: any) {
      message.error('查询配置态失败: ' + (err?.message || String(err)));
    } finally {
      setChecking(false);
    }
  };

  // 已配置 PAT：渲染 ActionButton 承载提交（清除 PAT 的入口在设置-第三方授权）。
  if (configured) {
    return (
      <ActionButton
        actionType={CONTRIBUTE_ACTION_TYPE}
        actionKey={expert.name}
        prompt={buildContributePrompt()}
        params={{
          expert_name: expert.name,
          version: expert.version,
          expert_dir: toHomePath(expert.definition_dir),
        }}
        icon={<ShareAltOutlined />}
        buttonType="text"
        buttonSize={size}
        workspaceId={state.selectedWorkspace ?? undefined}
        panelTitle={`分享专家 ${expert.name}`}
        panelDescription="AI 将读取本机 PAT，把该专家打包为 PR 提交到官方仓库（可编辑下方 Prompt）"
        // 完成态：提交 PR 后只有结果链接，无「应用/拒绝」概念，改为单个「确定」关闭。
        completedView={({ result, close }) => (
          <Space direction="vertical" size="middle" style={{ width: '100%' }}>
            <Text type="secondary">提交结果：</Text>
            <div
              style={{
                padding: 12,
                background: 'var(--color-success-bg, #f6ffed)',
                border: '1px solid var(--color-success-border, #b7eb8f)',
                borderRadius: 6,
                maxHeight: 400,
                overflow: 'auto',
              }}
            >
              <Paragraph style={{ whiteSpace: 'pre-wrap', margin: 0 }}>{result}</Paragraph>
            </div>
            <Button type="primary" block onClick={close}>
              确定
            </Button>
          </Space>
        )}
      >
        分享
      </ActionButton>
    );
  }

  // 未确认配置：渲染「分享」按钮（点击后引导去设置填写 PAT）。
  return (
    <Tooltip title="分享到官方仓库">
      <Button
        type="text"
        size={size}
        icon={<ShareAltOutlined />}
        onClick={handleClick}
        loading={checking}
        style={{ color: 'var(--color-text-secondary)' }}
      >
        分享
      </Button>
    </Tooltip>
  );
}
