// LinkSkillPicker.tsx
// ---------------------------------------------------------------------------
// 环节属性面板的技能选择（需求 037）。
// 复用 todo 侧的 SkillSelector 卡片 UI，但语义不同：
// - todo：点击技能把 /skill 插入 prompt（不存字段）
// - 环节：点击 toggle 选中，存 link.skills[]（运行时用，installer 写 loop_steps.skill_names）
// 按环节的 executor 加载该执行器的技能列表；未指定 executor 时不渲染。
// ---------------------------------------------------------------------------
import { useEffect, useState, type JSX } from 'react';
// 复用 todo 侧技能卡片组件（已加 selectedSkills 选中态，向后兼容 todo 用法）
import { SkillSelector } from '@/components/todo-drawer/SkillSelector';
import { getSkillsList } from '@/utils/database/skills';
import type { SkillMeta } from '@/types';

export interface LinkSkillPickerProps {
  // 环节填写的执行器名：用于加载该执行器的技能列表
  executor?: string;
  // 已选中的技能名（link.skills），传给 SkillSelector 显示选中态
  selectedSkills?: string[];
  // 技能卡片点击回调：调用方负责 toggle 写入 link.skills[]
  onToggleSkill: (skillName: string) => void;
}

// 按环节 executor 加载技能，复用 SkillSelector 卡片，点击 toggle 选中。
export function LinkSkillPicker({
  executor,
  selectedSkills,
  onToggleSkill,
}: LinkSkillPickerProps): JSX.Element | null {
  const [skills, setSkills] = useState<SkillMeta[]>([]);
  const [loading, setLoading] = useState(false);
  const [expanded, setExpanded] = useState(false);
  const [search, setSearch] = useState('');

  // executor 变化时重新加载该执行器的技能；
  // cancelled 标志防御快速切换 executor 时晚返回的请求覆盖新结果。
  useEffect(() => {
    if (!executor) {
      setSkills([]);
      return;
    }
    let cancelled = false;
    setLoading(true);
    getSkillsList()
      .then((list) => {
        if (cancelled) return;
        // 按环节 executor 过滤；找不到（executor 非已注册执行器）则空
        const found = list.find((e) => e.executor === executor);
        setSkills(found?.skills ?? []);
      })
      .catch(() => {
        if (!cancelled) setSkills([]);
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [executor]);

  // 未指定执行器：无可选技能，不渲染（避免"暂无 Skills"噪声）
  if (!executor) return null;

  return (
    <SkillSelector
      skills={skills}
      selectedSkills={selectedSkills}
      loading={loading}
      executorColor="var(--color-primary)"
      searchText={search}
      onSearchChange={setSearch}
      expanded={expanded}
      onToggle={() => setExpanded((prev) => !prev)}
      onSkillClick={(s) => onToggleSkill(s.name)}
    />
  );
}
