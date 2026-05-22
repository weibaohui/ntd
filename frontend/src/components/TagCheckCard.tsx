import { memo } from 'react';
import { CheckOutlined } from '@ant-design/icons';

interface Tag {
  id: number;
  name: string;
  color: string;
}

interface TagCheckCardProps {
  tag: Tag;
  selected: boolean;
  onClick: () => void;
}

const TagCheckCard = memo(function TagCheckCard({ tag, selected, onClick }: TagCheckCardProps) {
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
      style={{
        display: 'flex',
        alignItems: 'center',
        gap: 10,
        padding: '12px 16px',
        borderRadius: 12,
        border: `2px solid ${selected ? tag.color : 'var(--color-border)'}`,
        background: selected ? `${tag.color}10` : 'var(--color-bg-elevated)',
        cursor: 'pointer',
        transition: 'all 0.2s ease',
        position: 'relative',
        flex: '1 1 calc(50% - 8px)',
        minWidth: 120,
      }}
      onMouseEnter={(e) => {
        if (!selected) {
          (e.currentTarget as HTMLDivElement).style.borderColor = `${tag.color}60`;
          (e.currentTarget as HTMLDivElement).style.background = `${tag.color}08`;
        }
      }}
      onMouseLeave={(e) => {
        if (!selected) {
          (e.currentTarget as HTMLDivElement).style.borderColor = 'var(--color-border)';
          (e.currentTarget as HTMLDivElement).style.background = 'var(--color-bg-elevated)';
        }
      }}
    >
      <span
        style={{
          width: 10,
          height: 10,
          borderRadius: '50%',
          backgroundColor: tag.color,
          flexShrink: 0,
        }}
      />
      <span
        style={{
          fontSize: 14,
          fontWeight: 600,
          color: selected ? tag.color : 'var(--color-text)',
          flex: 1,
        }}
      >
        {tag.name}
      </span>
      {selected && (
        <span
          style={{
            width: 20,
            height: 20,
            borderRadius: '50%',
            backgroundColor: tag.color,
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            flexShrink: 0,
          }}
        >
          <CheckOutlined style={{ fontSize: 11, color: '#fff', fontWeight: 700 }} />
        </span>
      )}
    </div>
  );
});

interface TagCheckCardGroupProps {
  tags: Tag[];
  value: number | number[] | null;
  onChange: (value: number | number[] | null) => void;
  multiple?: boolean;
}

export function TagCheckCardGroup({ tags, value, onChange, multiple = false }: TagCheckCardGroupProps) {
  if (tags.length === 0) return null;

  const isSelected = (tagId: number) => {
    if (value === null) return false;
    if (multiple) return (value as number[]).includes(tagId);
    return value === tagId;
  };

  const handleClick = (tagId: number) => {
    if (multiple) {
      const current = (value as number[] | null) || [];
      if (current.includes(tagId)) {
        onChange(current.filter(id => id !== tagId));
      } else {
        onChange([...current, tagId]);
      }
    } else {
      onChange(isSelected(tagId) ? null : tagId);
    }
  };

  return (
    <div
      style={{
        display: 'flex',
        flexWrap: 'wrap',
        gap: 10,
      }}
    >
      {tags.map(tag => (
        <TagCheckCard
          key={tag.id}
          tag={tag}
          selected={isSelected(tag.id)}
          onClick={() => handleClick(tag.id)}
        />
      ))}
    </div>
  );
}
