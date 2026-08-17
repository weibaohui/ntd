import { useMemo, useState } from 'react';
import { Tag, Tooltip } from 'antd';
import { AppstoreOutlined } from '@ant-design/icons';
import { EXECUTORS } from '@/types';
import type { SkillMeta, ExecutorSkills } from '@/types';
import { EXECUTOR_COLORS, splitSkillName, formatSize } from './helpers';
import { ShareToRepoButton } from '@/components/settings/contribute/ShareToRepoButton';
import { buildSkillContributePrompt } from '@/components/settings/contribute/contributePrompts';
import './SkillCardView.css';

interface SkillCardViewProps {
  data: ExecutorSkills[];
  searchText: string;
  onSkillClick: (skill: SkillMeta, executor: string) => void;
}

// 执行器固定顺序（与 EXECUTORS 保持一致，但只取有 skills_dir 的执行器）
const EXECUTOR_ORDER = [
  'claudecode', 'codebuddy', 'opencode', 'mobilecoder', 'atomcode',
  'hermes', 'kimi', 'codex', 'pi', 'mimo', 'zhanlu', 'agents',
];

// 渐变背景色生成器，基于 skill 名称 hash
function generateGradient(name: string): string {
  let hash = 0;
  for (let i = 0; i < name.length; i++) {
    hash = ((hash << 5) - hash + name.charCodeAt(i)) | 0;
  }
  const h1 = Math.abs(hash) % 360;
  const h2 = (h1 + 40) % 360;
  return `linear-gradient(135deg, hsl(${h1}, 70%, 60%), hsl(${h2}, 60%, 50%))`;
}

// 执行器方块组件
function ExecutorBlock({ executor, installed }: { executor: string; installed: boolean }) {
  const color = EXECUTOR_COLORS[executor] || '#64748b';
  const label = EXECUTORS.find(e => e.value === executor)?.label || executor;

  return (
    <Tooltip title={`${label}${installed ? ' ✓' : ' 未安装'}`} placement="bottom">
      <div
        style={{
          width: 20,
          height: 20,
          borderRadius: 4,
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center',
          fontSize: 10,
          fontWeight: 600,
          color: installed ? '#fff' : color,
          background: installed ? color : 'transparent',
          border: installed ? 'none' : `1px dashed ${color}80`,
          opacity: installed ? 1 : 0.4,
          transition: 'all 0.2s',
          cursor: 'default',
          flexShrink: 0,
        }}
      >
        {label.charAt(0).toUpperCase()}
      </div>
    </Tooltip>
  );
}

// 单个 Skill 卡片
function SkillCard({ skill, executors, onClick, shareDir }: {
  skill: SkillMeta;
  executors: { name: string; installed: boolean }[];
  onClick: () => void;
  /** 技能目录（~ 相对路径，后端 skills_dir 已转 ~），空则不渲染分享入口 */
  shareDir: string;
}) {
  const { category, shortName } = splitSkillName(skill.name);
  const gradient = generateGradient(skill.name);

  // 计算已安装的执行器数量
  const installedCount = executors.filter(e => e.installed).length;

  return (
    <div
      onClick={onClick}
      role="button"
      tabIndex={0}
      onKeyDown={(e) => {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault();
          onClick();
        }
      }}
      className="skill-card-item"
    >
      {/* 头像区域：渐变背景 + 首字母 */}
      <div
        className="skill-card-avatar"
        style={{
          background: gradient,
          boxShadow: `0 4px 12px ${gradient.includes('hsl') ? 'rgba(0,0,0,0.15)' : gradient}`,
        }}
      >
        {shortName.charAt(0).toUpperCase()}
      </div>

      {/* 名称 */}
      <div className="skill-card-title">
        {shortName}
      </div>

      {/* 描述 */}
      {skill.description && (
        <div className="skill-card-description">
          {skill.description}
        </div>
      )}

      {/* 标签区域：category + version + 文件大小 */}
      <div className="skill-card-tags">
        {category && (
          <Tag style={{
            margin: 0,
            fontSize: 11,
            lineHeight: '18px',
            padding: '0 6px',
            borderRadius: 6,
            background: 'var(--color-fill, #f1f5f9)',
            border: 'none',
            color: 'var(--color-text-secondary, #64748b)',
          }}>
            {category}
          </Tag>
        )}
        {skill.version && (
          <Tag style={{
            margin: 0,
            fontSize: 11,
            lineHeight: '18px',
            padding: '0 6px',
            borderRadius: 6,
            background: 'rgba(8, 145, 178, 0.1)',
            border: 'none',
            color: '#0891b2',
          }}>
            v{skill.version}
          </Tag>
        )}
        <span style={{
          marginLeft: 'auto',
          fontSize: 11,
          color: 'var(--color-text-quaternary, #94a3b8)',
        }}>
          {formatSize(skill.total_size)}
        </span>
      </div>

      {/* 执行器方块区域 */}
      <div className="skill-card-executors">
        <div style={{
          flex: 1,
          display: 'flex',
          alignItems: 'center',
          gap: 4,
          flexWrap: 'wrap',
        }}>
          {executors.map(({ name, installed }) => (
            <ExecutorBlock
              key={name}
              executor={name}
              installed={installed}
            />
          ))}
        </div>
        <span style={{
          fontSize: 11,
          color: 'var(--color-text-tertiary, #94a3b8)',
        }}>
          {installedCount}/{executors.length}
        </span>
        {/* 分享到官方仓库：技能全量可分享（不区分来源）。
            卡片本身可点击打开详情，分享按钮需阻止冒泡，避免误开详情抽屉。 */}
        {shareDir && (
          <div
            onClick={(e) => e.stopPropagation()}
            role="presentation"
            style={{ display: 'flex', alignItems: 'center' }}
          >
            <ShareToRepoButton
              actionType="skill_contribute"
              params={{
                resource_name: skill.name,
                version: skill.version || '',
                resource_dir: shareDir,
                remote_path: `skills/${skill.name}/`,
              }}
              buildPrompt={buildSkillContributePrompt}
              panelTitle={`分享技能 ${skill.name}`}
              panelDescription="AI 将读取本机 PAT，把该技能目录提交为 PR 到官方仓库（可编辑下方 Prompt）"
              size="small"
              iconOnly
            />
          </div>
        )}
      </div>
    </div>
  );
}

// 卡片视图主组件
export function SkillCardView({ data, searchText, onSkillClick }: SkillCardViewProps) {
  const [selectedCategories, setSelectedCategories] = useState<string[]>([]);

  // 提取所有唯一的 category
  const allCategories = useMemo(() => {
    const categories = new Set<string>();
    data.forEach(e => {
      e.skills.forEach(s => {
        const { category } = splitSkillName(s.name);
        if (category) categories.add(category);
      });
    });
    return Array.from(categories).sort();
  }, [data]);

  // 从 data 中提取实际存在的执行器列表（按固定顺序）
  const activeExecutors = useMemo(() => {
    const executorSet = new Set(data.map(e => e.executor));
    return EXECUTOR_ORDER.filter(name => executorSet.has(name));
  }, [data]);

  // 去重后的 skill 列表，每个 skill 包含所有执行器的安装状态
  const dedupedSkills = useMemo(() => {
    // 按 skill name 去重，收集每个 skill 在哪些执行器上安装了
    const skillMap = new Map<string, {
      skill: SkillMeta;
      installedExecutors: Set<string>;
    }>();

    data.forEach(executor => {
      executor.skills.forEach(skill => {
        const existing = skillMap.get(skill.name);
        if (existing) {
          existing.installedExecutors.add(executor.executor);
        } else {
          skillMap.set(skill.name, {
            skill,
            installedExecutors: new Set([executor.executor]),
          });
        }
      });
    });

    return skillMap;
  }, [data]);

  // 过滤 + 按固定顺序补全执行器状态
  const filteredSkills = useMemo(() => {
    let entries = Array.from(dedupedSkills.entries());

    // 按搜索文本过滤
    if (searchText) {
      const lower = searchText.toLowerCase();
      entries = entries.filter(([_, { skill }]) =>
        skill.name.toLowerCase().includes(lower) ||
        skill.description?.toLowerCase().includes(lower) ||
        skill.keywords?.some(k => k.toLowerCase().includes(lower))
      );
    }

    // 按 category 过滤
    if (selectedCategories.length > 0) {
      entries = entries.filter(([_, { skill }]) => {
        const { category } = splitSkillName(skill.name);
        return selectedCategories.includes(category || '未分类');
      });
    }

    // 为每个 skill 构建固定顺序的执行器列表
    return entries.map(([_, { skill, installedExecutors }]) => {
      const executors = activeExecutors.map(name => ({
        name,
        installed: installedExecutors.has(name),
      }));
      // 找该技能首个安装执行器的 skills_dir（后端已转 ~ 相对），拼接出技能目录供分享定位
      const owner = data.find(e => e.skills.some(s => s.name === skill.name));
      const skillsDir = owner?.skills_dir ? `${owner.skills_dir}/${skill.name}` : '';
      return { skill, executors, skillsDir };
    });
  }, [dedupedSkills, searchText, selectedCategories, activeExecutors]);

  // 切换 category 选择
  const toggleCategory = (category: string) => {
    setSelectedCategories(prev =>
      prev.includes(category)
        ? prev.filter(c => c !== category)
        : [...prev, category]
    );
  };

  return (
    <div className="skill-card-view">
      {/* 标签云筛选 */}
      {allCategories.length > 0 && (
        <div className="skill-category-cloud" style={{ marginBottom: 16 }}>
          <span style={{
            fontSize: 12,
            color: 'var(--color-text-tertiary, #94a3b8)',
            flexShrink: 0,
          }}>
            分类:
          </span>
          {allCategories.map(category => {
            const isSelected = selectedCategories.includes(category);
            return (
              <button
                key={category}
                onClick={() => toggleCategory(category)}
                className={`skill-category-btn ${isSelected ? 'skill-category-btn--active' : ''}`}
              >
                {category}
              </button>
            );
          })}
        </div>
      )}

      {/* 卡片网格 */}
      {filteredSkills.length === 0 ? (
        <div className="skill-empty-state">
          <AppstoreOutlined className="skill-empty-state-icon" />
          <div style={{ fontSize: 16 }}>{searchText ? '无匹配结果' : '暂无 Skills'}</div>
        </div>
      ) : (
        <div className="skill-card-grid">
          {filteredSkills.map(({ skill, executors, skillsDir }) => (
            <SkillCard
              key={skill.name}
              skill={skill}
              executors={executors}
              shareDir={skillsDir}
              onClick={() => onSkillClick(skill, executors.find(e => e.installed)?.name || data[0]?.executor || '')}
            />
          ))}
        </div>
      )}
    </div>
  );
}
