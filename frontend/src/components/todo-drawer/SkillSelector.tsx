import { Input, Tag, Spin, Empty } from 'antd';
import { RightOutlined, ThunderboltOutlined, SearchOutlined } from '@ant-design/icons';
import { useDeferredValue } from 'react';
import type { SkillMeta } from '../../types';

export function SkillSelector({ skills, loading, executorColor, searchText, onSearchChange, expanded, onToggle, onSkillClick }: {
  skills: SkillMeta[];
  loading: boolean;
  executorColor: string;
  searchText: string;
  onSearchChange: (v: string) => void;
  expanded: boolean;
  onToggle: () => void;
  onSkillClick: (skill: SkillMeta) => void;
}) {
  const deferredSearch = useDeferredValue(searchText);
  const filtered = deferredSearch.trim()
    ? skills.filter(s =>
        s.name.toLowerCase().includes(deferredSearch.toLowerCase()) ||
        s.description?.toLowerCase().includes(deferredSearch.toLowerCase()) ||
        s.keywords?.some(k => k.toLowerCase().includes(deferredSearch.toLowerCase()))
      )
    : skills;

  if (loading && skills.length === 0) {
    return (
      <div style={{ textAlign: 'center', padding: 16 }}>
        <Spin size="small" />
      </div>
    );
  }

  if (!loading && skills.length === 0) {
    return null;
  }

  return (
    <div style={{ marginBottom: 16 }}>
      <div
        onClick={onToggle}
        role="button"
        tabIndex={0}
        onKeyDown={(e) => {
          if (e.key === 'Enter' || e.key === ' ') {
            e.preventDefault();
            onToggle();
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
        <RightOutlined style={{
          color: executorColor,
          fontSize: 10,
          marginRight: 6,
          transition: 'transform 0.2s',
          transform: expanded ? 'rotate(90deg)' : 'rotate(0deg)',
        }} />
        <ThunderboltOutlined style={{ color: executorColor, marginRight: 6 }} />
        Skills
        <span style={{ fontWeight: 400, fontSize: 12, color: 'var(--color-text-tertiary)', marginLeft: 8 }}>
          {filtered.length}{deferredSearch ? `/${skills.length}` : ''} 个可用
        </span>
      </div>
      {expanded && (
        <div style={{ marginBottom: 10 }}>
          <Input
            prefix={<SearchOutlined style={{ color: 'var(--color-text-quaternary)' }} />}
            placeholder="搜索 Skills..."
            value={searchText}
            onChange={(e) => onSearchChange(e.target.value)}
            allowClear
            style={{ width: '100%' }}
          />
        </div>
      )}
      {expanded && filtered.length > 0 && (
        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(2, 1fr)', gap: 10 }}>
          {filtered.map(skill => (
            <div
              key={skill.name}
              onClick={() => onSkillClick(skill)}
              role="button"
              tabIndex={0}
              onKeyDown={(e) => {
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
                (e.currentTarget as HTMLDivElement).style.borderColor = executorColor;
                (e.currentTarget as HTMLDivElement).style.background = `${executorColor}08`;
              }}
              onMouseLeave={(e) => {
                (e.currentTarget as HTMLDivElement).style.borderColor = 'var(--color-border-secondary)';
                (e.currentTarget as HTMLDivElement).style.background = 'var(--color-bg-elevated)';
              }}
            >
              <div style={{ fontSize: 13, fontWeight: 600, color: 'var(--color-text)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                {skill.name}
              </div>
              {skill.description && (
                <div style={{ fontSize: 11, color: 'var(--color-text-tertiary)', marginTop: 4, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                  {skill.description}
                </div>
              )}
              <div style={{ display: 'flex', flexWrap: 'wrap', gap: 4, marginTop: 6, alignItems: 'center' }}>
                {skill.version && (
                  <Tag style={{ fontSize: 10, lineHeight: '16px', padding: '0 4px', margin: 0 }} color="blue">v{skill.version}</Tag>
                )}
                {skill.author && (
                  <span style={{ fontSize: 10, color: 'var(--color-text-quaternary)' }}>{skill.author}</span>
                )}
                {skill.file_count > 0 && (
                  <span style={{ fontSize: 10, color: 'var(--color-text-quaternary)', marginLeft: 'auto' }}>
                    {skill.file_count} 文件
                  </span>
                )}
              </div>
            </div>
          ))}
        </div>
      )}
      {expanded && filtered.length === 0 && deferredSearch && (
        <div style={{ textAlign: 'center', padding: '16px 0', color: 'var(--color-text-tertiary)' }}>
          未找到匹配 "{deferredSearch}" 的 Skill
        </div>
      )}
      {!loading && skills.length === 0 && (
        <div style={{ marginBottom: 16 }}>
          <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description="当前执行器暂无 Skills" style={{ margin: 0 }} />
        </div>
      )}
    </div>
  );
}
