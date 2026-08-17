// 通用「分享到官方仓库」按钮：PAT 配置引导 + ActionButton 提示词驱动提交 PR。
// 从 experts/ContributeButton 抽取通用逻辑，供专家/工艺/事项模板/技能四类资源复用。
// 差异只在参数（params/actionType/提示词/面板文案）与可选 onPrepare（分享前先准备文件，如事项模板导出 YAML）。
// PAT 的填写/保存/清空统一收口到「设置 → 第三方授权 → GitCode」，未配置 PAT 时跳转设置引导填写。

import { useState } from 'react';
import { App, Button, Space, Tooltip, Typography } from 'antd';
import { ShareAltOutlined } from '@ant-design/icons';
import { ActionButton } from '@/components/ActionButton';
import { useTodos } from '@/hooks/useTodoContext';
import { useViewState } from '@/hooks/useViewState';
import { getContributionAuthStatus } from '@/utils/database/contribution';
import { THIRD_PARTY_SETTINGS_TAB } from '@/components/settings/ThirdPartyPanel';

// 113：分享类 action 统一固定 actionKey（与标题优化 title_optimize/default 同口径）——
// 一类分享只占一个事项（expert/process/skill/todo_contribute 各 1 个），
// 重复分享只新增执行记录，资源区分靠 params（resource_name/remote_path），不再按资源建事项。
const SHARE_ACTION_KEY = 'default';

/**
 * 把定义目录的绝对路径转成 ~/ 相对路径，避免在 prompt/执行记录里暴露家目录下的用户名。
 * 资源目录都在 ~/.ntd/ 下（~/.ntd/experts/、~/.ntd/processes/、~/.ntd/bundled/ 等）。
 */
export function toHomePath(absPath: string): string {
  const marker = '/.ntd/';
  const idx = absPath.indexOf(marker);
  // 找到 /.ntd/ 就把其前缀（家目录）替换为 ~；找不到则原样返回（理论上不会发生）。
  return idx >= 0 ? `~${absPath.slice(idx)}` : absPath;
}

/**
 * 分享按钮组件（通用）。
 *
 * 交互流程：
 * 1. 点击「分享」→（可选 onPrepare 准备文件）→ 查询 PAT 配置态：未配置 → 提示并跳转设置-第三方授权；已配置 → 切换为 ActionButton。
 * 2. ActionButton：打开 Drawer，展示/编辑提交 prompt（params 已注入占位符），选执行器后由 AI 执行（读取本机 PAT、调 GitCode API 完成 fork → 建分支 → 写文件 → 建 PR）。
 */
export function ShareToRepoButton({
  actionType,
  params,
  buildPrompt,
  panelTitle,
  panelDescription,
  onPrepare,
  size = 'middle',
  iconOnly = false,
}: {
  /** ActionButton action_type（按资源区分：expert/process/todo/skill_contribute） */
  actionType: string;
  /** 提示词占位符参数（{{key}} 替换），与 buildPrompt 的占位符一一对应 */
  params: Record<string, string>;
  /** 提示词构建函数（按资源类型），在渲染 ActionButton 时调用 */
  buildPrompt: () => string;
  /** ActionButton 面板标题 */
  panelTitle: string;
  /** ActionButton 面板说明 */
  panelDescription: string;
  /** 可选：点击分享后先执行的准备动作（如事项模板导出 YAML），返回的键值会合并进 params */
  onPrepare?: () => Promise<Record<string, string> | undefined>;
  size?: 'small' | 'middle';
  /** 仅图标模式（表格行内用，与其它操作按钮统一为 icon）；默认带「分享」文字（详情页等场景） */
  iconOnly?: boolean;
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
  // onPrepare 的产出（如事项模板导出后的文件路径），与 params 合并后作为最终占位符参数。
  const [prepared, setPrepared] = useState<Record<string, string> | null>(null);
  // 首次点击确认 PAT 已配置后置 true：让切换出来的 ActionButton 挂载后自动打开 Drawer，
  // 避免「第一次点击只查配置、第二次才弹抽屉」的割裂交互。
  const [autoOpen, setAutoOpen] = useState(false);

  // 点击分享：先（可选）准备文件，再查 PAT 配置态，已配置切到 ActionButton，未配置引导去设置填写。
  const handleClick = async () => {
    setChecking(true);
    try {
      // 准备动作失败（如导出接口报错）直接中断，不进入 PAT 检查流程
      if (onPrepare) {
        const extra = await onPrepare();
        if (extra) setPrepared(extra);
      }
      const status = await getContributionAuthStatus();
      if (status.configured) {
        setConfigured(true);
        // 切到 ActionButton 分支后让它挂载即打开 Drawer，首次点击直接看到提交面板
        setAutoOpen(true);
      } else {
        // PAT 管理已收口到设置：这里只给提示并跳转，不再内嵌填写框。
        message.info('请先在「设置 → 第三方授权 → GitCode」中填写并保存 PAT');
        showView('settings', { tab: THIRD_PARTY_SETTINGS_TAB });
      }
    } catch (err: any) {
      message.error('分享准备失败: ' + (err?.message || String(err)));
    } finally {
      setChecking(false);
    }
  };

  // 合并初始参数与 onPrepare 产出：onPrepare 的键优先（如 todo 导出后的目录覆盖默认空值）。
  const finalParams = { ...params, ...prepared };

  // 已配置 PAT：渲染 ActionButton 承载提交（清除 PAT 的入口在设置-第三方授权）。
  if (configured) {
    return (
      <ActionButton
        actionType={actionType}
        // 113：固定 key，一类分享共用一个事项；actionType 负责区分四类资源
        actionKey={SHARE_ACTION_KEY}
        prompt={buildPrompt()}
        params={finalParams}
        icon={<ShareAltOutlined />}
        buttonType="text"
        buttonSize={size}
        // iconOnly 时隐藏按钮文字：ActionButton 在 children 为空时兜底显示「优化标题」（历史默认文案），
        // 分享场景必须用 showLabel=false 关掉，否则表格里的分享图标会变成「优化标题」文字按钮。
        showLabel={!iconOnly}
        // 首次点击查完 PAT 后自动打开 Drawer（autoOpen 只在挂载时触发一次）
        autoOpen={autoOpen}
        workspaceId={state.selectedWorkspace ?? undefined}
        panelTitle={panelTitle}
        panelDescription={panelDescription}
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
        {/* iconOnly 时按钮纯图标（表格行内统一风格），文字模式保留「分享」便于详情页语义明确 */}
        {iconOnly ? null : '分享'}
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
        {/* iconOnly 时按钮纯图标，文字模式保留「分享」 */}
        {iconOnly ? null : '分享'}
      </Button>
    </Tooltip>
  );
}