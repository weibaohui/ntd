// ─── 关联专家徽章 ────────────────────────────────────────────
//
// 在 Todo 详情头部展示当前 Todo 关联的专家/团队信息：
// - 视觉风格与 ExecutorBadge 对齐（小号色块 + 主色文字）。
// - 通过 getExpertByName 按名拉取，避免拉全列表造成的额外开销。
// - 加载失败/未找到时静默不渲染，保持头部整洁。
// - 用 Tooltip 暴露职业与描述，避免头部信息密度过高。

import { useEffect, useState } from 'react';
import { Tag, Tooltip } from 'antd';
import { TeamOutlined, UserOutlined } from '@ant-design/icons';
import * as db from '@/utils/database';
import type { ExpertMetadata } from '@/types/expert';
import {
  getExpertDisplayName,
  getExpertProfession,
  getExpertDescription,
} from '@/types/expert';

// 与 ExecutorBadge 视觉风格保持一致的徽章基础样式：
// 选用 info 色板，避免与执行器徽章色彩冲突，同时保持低饱和背景突出文字。
const BADGE_BASE_STYLE: React.CSSProperties = {
  // 背景使用 info 色板的浅色调，呼应 Ant Design 语义色板。
  backgroundColor: 'var(--color-info-bg-1)',
  // 文字色采用 info 主色，保证在浅色背景上的可读性。
  color: 'var(--color-info)',
  // 用户偏好无边框，避免与 ExecutorBadge 的有色边框混淆角色。
  border: 'none',
  // 使用 inline-flex 让图标与文字垂直居中对齐。
  display: 'inline-flex',
  alignItems: 'center',
  gap: 4,
  // 尺寸与 ExecutorBadge 完全一致，保证视觉对齐。
  padding: '2px 8px',
  borderRadius: 4,
  fontSize: 11,
  fontWeight: 600,
  // Tag 默认带外边距，会破坏 flex 布局对齐，这里清零。
  margin: 0,
};

interface ExpertBadgeProps {
  // 关联专家名称，对应 ExpertMetadata.name，作为数据加载的键。
  expertName: string;
  // 可选的点击跳转回调：未来接入专家详情页时由父组件传入，避免组件耦合路由。
  onClick?: (expertName: string) => void;
  // 自定义类名，便于父组件覆盖样式。
  className?: string;
  // 自定义内联样式，与 ExecutorBadge 接口保持一致。
  style?: React.CSSProperties;
}

/**
 * 按 expertName 异步加载单个专家元数据。
 *
 * 拆分为独立 hook 的原因：
 * 1. 让 ExpertBadge 主函数体保持在 30 行以内。
 * 2. 用 cancelled 标志防御快速切换 Todo 时的竞态，避免晚返回的旧数据覆盖新数据。
 */
function useExpertLoader(expertName: string) {
  // 当前加载到的专家数据，null 表示未找到或加载失败。
  const [expert, setExpert] = useState<ExpertMetadata | null>(null);
  // 是否已完成加载（成功或失败），用于决定是否渲染。
  const [loaded, setLoaded] = useState(false);

  useEffect(() => {
    // 闭包内的取消标志：组件卸载或 expertName 变化时置为 true，丢弃过期结果。
    let cancelled = false;
    // 通过 getExpertByName 拉单条记录，比 getAllExperts 更省流量。
    db.getExpertByName(expertName)
      .then((data) => {
        // 仅在未取消时写入，避免竞态。
        if (!cancelled) setExpert(data);
      })
      .catch(() => {
        // 静默失败：找不到专家时不渲染徽章，不打扰用户。
        if (!cancelled) setExpert(null);
      })
      .finally(() => {
        // 标记加载完成，触发首次渲染判断。
        if (!cancelled) setLoaded(true);
      });
    // 清理函数：切换 expertName 或卸载时取消未完成请求的结果写入。
    return () => { cancelled = true; };
  }, [expertName]);

  return { expert, loaded };
}

/**
 * 从 ExpertMetadata 计算用于展示的字段，避免主组件内堆砌临时变量。
 */
function useExpertDisplayInfo(expert: ExpertMetadata) {
  return {
    // team 类型用团队图标，agent 类型用单人图标，让用户一眼区分。
    isTeam: expert.expert_type === 'team',
    // 中文名优先，回退英文，最后回退 name 标识。
    displayName: getExpertDisplayName(expert),
    // 把职业和描述拼成 Tooltip 文案，缺省项自动过滤，避免出现 " · "。
    tipContent: [
      getExpertProfession(expert),
      getExpertDescription(expert),
    ].filter(Boolean).join(' · '),
  };
}

/**
 * 渲染关联专家徽章。
 *
 * 设计取舍：
 * - 未加载完成或加载失败时返回 null，避免头部出现空白占位或闪烁。
 * - 用 Tag + Tooltip 组合：Tag 承担视觉标识，Tooltip 承担详情展示，无需独立详情页。
 */
export function ExpertBadge({ expertName, onClick, className, style }: ExpertBadgeProps) {
  // 异步加载专家数据，loaded 用于判断是否已收到首次响应。
  const { expert, loaded } = useExpertLoader(expertName);
  // 加载未完成或未找到专家时静默不渲染。
  if (!loaded || !expert) return null;
  // 计算展示用的派生字段。
  const { isTeam, displayName, tipContent } = useExpertDisplayInfo(expert);

  return (
    <Tooltip title={tipContent || displayName}>
      <Tag
        className={className}
        style={{
          // 合并基础样式，保证视觉一致性。
          ...BADGE_BASE_STYLE,
          // 仅当父组件提供 onClick 时才显示手型指针，提示可点击。
          cursor: onClick ? 'pointer' : 'default',
          // 允许父组件通过 style 覆盖任何字段。
          ...style,
        }}
        // 点击时回调父组件传入的跳转函数；无回调时 noop。
        onClick={() => onClick?.(expertName)}
      >
        {/* 团队与单个专家使用不同图标，强化类型辨识。 */}
        {isTeam ? <TeamOutlined /> : <UserOutlined />}
        {displayName}
      </Tag>
    </Tooltip>
  );
}
