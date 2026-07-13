// ─── 专家技能选择器 ──────────────────────────────────────────
//
// 在 TodoDrawer 中选中专家后，展示该专家关联的技能列表。
// 数据来源：后端 /api/experts/:name/skills 接口，返回 SkillMetadata[]。
// 与执行器版的 SkillSelector 不同，这里聚焦"专家绑定了哪些技能"，
// 由组件内部自行管理加载状态与折叠状态，父组件只需传入 expertName。
//
// 设计取舍：
// - 不复用 SkillSelector，因为两者数据结构（SkillMetadata vs SkillMeta）、
//   数据来源（按专家拉取 vs 全局执行器技能）、生命周期都不同，强行复用会引入耦合。
// - 折叠状态内置而非受控，减少父组件状态负担（任务要求"内部自行管理"）。

import { memo, useState, useEffect, useCallback } from 'react';
import { Spin } from 'antd';
import { RightOutlined, StarOutlined } from '@ant-design/icons';
import * as db from '@/utils/database';
import type { SkillMetadata } from '@/types/expert';

/** 组件 Props */
export interface ExpertSkillSelectorProps {
  /** 当前选中的专家名称；为空时不渲染 */
  expertName?: string | null;
  /** 点击技能卡片回调，父组件负责将 /skill_name 插入到 prompt 编辑器 */
  onSkillClick: (skill: SkillMetadata) => void;
}

/**
 * 获取技能展示名称。
 * yaml_name 是 SKILL.md 中 YAML front matter 的 name 字段（更友好），
 * 缺失时回退到 skill_name（目录名），保证总有可读文本。
 */
function getSkillDisplayName(skill: SkillMetadata): string {
  // 优先用 YAML 中声明的 name（通常更人类可读），回退到目录名
  return skill.yaml_name || skill.skill_name;
}

/**
 * 获取技能描述（中文优先，回退英文）。
 * 与专家系统的多语言策略保持一致：zh 优先，en 兜底。
 */
function getSkillDescription(skill: SkillMetadata): string {
  // 中文描述优先，满足中文 UI 文案要求；缺失时回退英文描述
  return skill.yaml_description_zh || skill.yaml_description || '';
}

/**
 * 加载专家关联技能的自定义 Hook。
 *
 * 使用 cancelled 标志防御快速切换专家时的竞态：
 * 晚返回的请求若发现 expertName 已变，直接丢弃结果，避免渲染错位技能。
 */
function useExpertSkills(expertName?: string | null) {
  const [skills, setSkills] = useState<SkillMetadata[]>([]);
  const [loading, setLoading] = useState(false);

  // expertName 变化时重新拉取；空值时清空列表
  useEffect(() => {
    // 未选专家时清空历史数据，避免残留上一位专家的技能
    if (!expertName) {
      setSkills([]);
      return;
    }

    // cancelled 用于在 cleanup 阶段标记"本次请求已过期"
    let cancelled = false;
    setLoading(true);
    db.getExpertSkills(expertName)
      .then((data) => {
        // 仅当未被取消时才写入状态，防止竞态
        if (!cancelled) setSkills(data);
      })
      .catch(() => {
        // 加载失败静默清空，与 ExpertPicker 的错误处理策略一致
        if (!cancelled) setSkills([]);
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });

    // cleanup：expertName 变化或组件卸载时标记过期
    return () => {
      cancelled = true;
    };
  }, [expertName]);

  return { skills, loading };
}

/**
 * 渲染单个技能卡片。
 * 抽离为独立函数以满足"单函数体不超过 30 行"的规范，并让主组件聚焦编排。
 */
function renderSkillCard(skill: SkillMetadata, onSkillClick: (skill: SkillMetadata) => void) {
  const name = getSkillDisplayName(skill);
  const desc = getSkillDescription(skill);

  // 键名用 skill_name（目录名，唯一），而非 yaml_name（可能重复或缺失）
  return (
    <div
      key={skill.skill_name}
      onClick={() => onSkillClick(skill)}
      role="button"
      tabIndex={0}
      onKeyDown={(e) => {
        // 支持键盘触发，满足无障碍要求
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault();
          onSkillClick(skill);
        }
      }}
      style={{
        padding: '10px 12px',
        borderRadius: 8,
        border: '1px solid var(--color-border-secondary)',
        background: 'var(--color-bg-elevated)',
        cursor: 'pointer',
        transition: 'all 0.2s ease',
        overflow: 'hidden',
      }}
      onMouseEnter={(e) => {
        // 悬停高亮：用主题色边框 + 极淡背景，与 SkillSelector 视觉风格统一
        const el = e.currentTarget as HTMLDivElement;
        el.style.borderColor = 'var(--color-primary)';
        el.style.background = 'var(--color-primary-bg-1)';
      }}
      onMouseLeave={(e) => {
        // 离开恢复默认样式
        const el = e.currentTarget as HTMLDivElement;
        el.style.borderColor = 'var(--color-border-secondary)';
        el.style.background = 'var(--color-bg-elevated)';
      }}
    >
      <div style={{ fontSize: 13, fontWeight: 600, color: 'var(--color-text)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
        {/* emoji 来自 YAML front matter，存在则前置展示，增强辨识度 */}
        {skill.yaml_emoji && <span style={{ marginRight: 4 }}>{skill.yaml_emoji}</span>}
        {name}
      </div>
      {desc && (
        <div style={{ fontSize: 11, color: 'var(--color-text-tertiary)', marginTop: 4, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
          {desc}
        </div>
      )}
      {skill.yaml_allowed_tools.length > 0 && (
        <div style={{ fontSize: 10, color: 'var(--color-text-quaternary)', marginTop: 6 }}>
          工具: {skill.yaml_allowed_tools.join(', ')}
        </div>
      )}
    </div>
  );
}

/**
 * 专家技能选择器（可折叠）。
 *
 * 渲染规则：
 * - 未选择专家 → 不渲染
 * - 加载中 → 显示 Spin
 * - 加载完成但无技能 → 不渲染（避免空块占用布局空间）
 * - 有技能 → 显示可折叠列表
 */
export const ExpertSkillSelector = memo(function ExpertSkillSelector({
  expertName,
  onSkillClick,
}: ExpertSkillSelectorProps) {
  const { skills, loading } = useExpertSkills(expertName);
  const [expanded, setExpanded] = useState(true);

  // 折叠切换回调，用 useCallback 固定引用避免子树无谓重渲染
  const handleToggle = useCallback(() => {
    setExpanded((prev) => !prev);
  }, []);

  // 未选专家时不渲染任何内容
  if (!expertName) return null;

  // 加载中显示居中 Spin
  if (loading) {
    return (
      <div style={{ textAlign: 'center', padding: 16 }}>
        <Spin size="small" />
      </div>
    );
  }

  // 无技能时不渲染，保持页面整洁
  if (skills.length === 0) return null;

  return (
    <div style={{ marginBottom: 16 }}>
      {/* 可折叠标题区：点击切换展开/收起 */}
      <div
        onClick={handleToggle}
        role="button"
        tabIndex={0}
        onKeyDown={(e) => {
          // 键盘可达性：Enter/Space 触发折叠
          if (e.key === 'Enter' || e.key === ' ') {
            e.preventDefault();
            handleToggle();
          }
        }}
        style={{
          marginBottom: expanded ? 10 : 0,
          fontWeight: 600,
          fontSize: 14,
          cursor: 'pointer',
          display: 'flex',
          alignItems: 'center',
          userSelect: 'none',
        }}
      >
        {/* 箭头图标随展开状态旋转，提供视觉反馈 */}
        <RightOutlined style={{
          color: 'var(--color-primary)',
          fontSize: 10,
          marginRight: 6,
          transition: 'transform 0.2s',
          transform: expanded ? 'rotate(90deg)' : 'rotate(0deg)',
        }} />
        {/* 星形图标区分"专家技能"与执行器的"Skills"区块 */}
        <StarOutlined style={{ color: 'var(--color-primary)', marginRight: 6 }} />
        专家技能
        <span style={{ fontWeight: 400, fontSize: 12, color: 'var(--color-text-tertiary)', marginLeft: 8 }}>
          {skills.length} 个可用
        </span>
      </div>

      {/* 展开时渲染技能卡片网格，双列布局与 SkillSelector 保持一致 */}
      {expanded && (
        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(2, 1fr)', gap: 10 }}>
          {skills.map((skill) => renderSkillCard(skill, onSkillClick))}
        </div>
      )}
    </div>
  );
});
