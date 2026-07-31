// skillSelectionUtils.test.ts
// 环节 Skills 选择器桥接纯函数的单测（需求 053）。
import { describe, it, expect } from 'vitest';
import {
  splitSelected,
  syncFromTable,
  canAddCustom,
  addCustom,
  removeSkill,
  filterSkills,
  skillTagMeta,
} from './skillSelectionUtils';
import type { SkillMeta } from '@/types';

// SkillMeta 字段较多，用工厂补齐默认值，测试只关心匹配用到的字段。
const makeSkill = (over: Partial<SkillMeta>): SkillMeta => ({
  name: '',
  description: '',
  version: null,
  author: null,
  license: null,
  keywords: [],
  file_count: 0,
  total_size: 0,
  modified_at: null,
  ...over,
});

describe('splitSelected', () => {
  it('拆分列表内选中与手填自定义', () => {
    const r = splitSelected(['a', 'b', 'x'], new Set(['a', 'b', 'c']));
    expect(r.inList).toEqual(['a', 'b']);
    expect(r.custom).toEqual(['x']);
  });
  it('undefined 已选返回空', () => {
    const r = splitSelected(undefined, new Set(['a']));
    expect(r.inList).toEqual([]);
    expect(r.custom).toEqual([]);
  });
  it('全部在列表内时 custom 为空', () => {
    const r = splitSelected(['a', 'b'], new Set(['a', 'b', 'c']));
    expect(r.inList).toEqual(['a', 'b']);
    expect(r.custom).toEqual([]);
  });
  it('全部为自定义时 inList 为空', () => {
    const r = splitSelected(['x', 'y'], new Set(['a', 'b']));
    expect(r.inList).toEqual([]);
    expect(r.custom).toEqual(['x', 'y']);
  });
});

describe('syncFromTable', () => {
  it('保留手填项并同步表格勾选', () => {
    // a 是手填(不在列表)，b/c 在列表；用户在表格只勾 b → 结果含 a、b，不含 c
    const r = syncFromTable(['a', 'b', 'c'], new Set(['b', 'c']), ['b']);
    expect(r).toEqual(['a', 'b']);
  });
  it('表格全清空时仅保留手填项', () => {
    const r = syncFromTable(['a', 'b'], new Set(['b']), []);
    expect(r).toEqual(['a']);
  });
  it('手填项顺序保持在勾选项之前', () => {
    const r = syncFromTable(['x', 'y'], new Set(['a', 'b']), ['a', 'b']);
    expect(r).toEqual(['x', 'y', 'a', 'b']);
  });
  it('勾选项去重（后端可能返回重复 skill 名，如 claudecode 两个 code-refactoring）', () => {
    const r = syncFromTable([], new Set(['code-refactoring']), ['code-refactoring', 'code-refactoring']);
    expect(r).toEqual(['code-refactoring']);
  });
  it('custom 与勾选项同名时去重', () => {
    const r = syncFromTable(['x'], new Set(['x']), ['x']);
    expect(r).toEqual(['x']);
  });
});

describe('canAddCustom', () => {
  it('非空且不在列表且未选 → 可加', () => {
    expect(canAddCustom(['a'], new Set(['b']), 'c')).toBe(true);
  });
  it('空白关键字 → 不可加', () => {
    expect(canAddCustom([], new Set(['a']), '   ')).toBe(false);
  });
  it('列表内已存在(精确) → 不可加', () => {
    expect(canAddCustom([], new Set(['deploy']), 'deploy')).toBe(false);
  });
  it('列表内已存在(大小写不敏感) → 不可加', () => {
    expect(canAddCustom([], new Set(['Deploy']), 'deploy')).toBe(false);
  });
  it('已选中已存在(大小写不敏感) → 不可加', () => {
    expect(canAddCustom(['Deploy'], new Set(['a']), 'deploy')).toBe(false);
  });
});

describe('addCustom', () => {
  it('追加新项到末尾', () => {
    expect(addCustom(['a'], 'b')).toEqual(['a', 'b']);
  });
  it('去重(大小写不敏感)，保留原有大小写', () => {
    expect(addCustom(['Deploy'], 'deploy')).toEqual(['Deploy']);
  });
  it('trim 空格后追加', () => {
    expect(addCustom(['a'], '  b  ')).toEqual(['a', 'b']);
  });
  it('空字符串不追加', () => {
    expect(addCustom(['a'], '   ')).toEqual(['a']);
  });
  it('undefined 已选 → 单元素数组', () => {
    expect(addCustom(undefined, 'a')).toEqual(['a']);
  });
});

describe('removeSkill', () => {
  it('精确移除指定项', () => {
    expect(removeSkill(['a', 'b', 'c'], 'b')).toEqual(['a', 'c']);
  });
  it('不存在则原样返回', () => {
    expect(removeSkill(['a', 'b'], 'x')).toEqual(['a', 'b']);
  });
  it('undefined 已选 → 空', () => {
    expect(removeSkill(undefined, 'a')).toEqual([]);
  });
});

describe('filterSkills', () => {
  const skills: SkillMeta[] = [
    makeSkill({ name: 'deploy', description: '部署到生产', keywords: ['cd'] }),
    makeSkill({ name: 'lint', description: 'code check', keywords: ['quality'] }),
  ];
  it('空关键字返回全部', () => {
    expect(filterSkills(skills, '')).toHaveLength(2);
  });
  it('按 name 匹配', () => {
    expect(filterSkills(skills, 'dep')).toHaveLength(1);
  });
  it('按 description 匹配(中文)', () => {
    expect(filterSkills(skills, '部署')).toHaveLength(1);
  });
  it('按 keywords 匹配', () => {
    expect(filterSkills(skills, 'quality')).toHaveLength(1);
  });
  it('大小写不敏感', () => {
    expect(filterSkills(skills, 'DEP')).toHaveLength(1);
  });
  it('无匹配返回空数组', () => {
    expect(filterSkills(skills, 'zzz')).toEqual([]);
  });
});

describe('skillTagMeta', () => {
  const src = new Map<string, string[]>([
    ['deploy', ['claudecode']],
    ['shared', ['claudecode', 'pi']],
  ]);
  it('已知 skill（仅其他执行器有）→ 蓝无标注', () => {
    expect(skillTagMeta('deploy', src)).toEqual({ color: 'blue', suffix: '' });
  });
  it('多执行器都有的已知 skill → 蓝无标注', () => {
    expect(skillTagMeta('shared', src)).toEqual({ color: 'blue', suffix: '' });
  });
  it('全量都没有 → 橙自定义', () => {
    expect(skillTagMeta('my-skill', src)).toEqual({ color: 'orange', suffix: ' ·自定义' });
  });
});
