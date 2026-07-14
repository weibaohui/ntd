// 单个专家卡片：展示头像、名称、职业、描述、标签、技能数，悬停上浮高亮。
// 从 ExpertsPanel 拆出；蓝色系，与 TeamCard（橙色系）区分专家类型。

import { useState } from 'react';
import { Tag, Typography } from 'antd';
import { UserOutlined, ThunderboltOutlined, RightOutlined } from '@ant-design/icons';
import type { ExpertMetadata } from '@/types/expert';
import {
  getExpertDisplayName,
  getExpertProfession,
  getExpertDescription,
  getExpertAvatarUrl,
  getCategoryName,
} from '@/types/expert';

const { Text } = Typography;

/**
 * 单个专家卡片组件
 *
 * 展示头像、名称、职业、描述、标签、技能数等信息。
 * 卡片悬停时有上浮效果和边框高亮。
 */
export function ExpertCard({ expert, onClick }: {
  expert: ExpertMetadata;
  onClick: (expert: ExpertMetadata) => void;
}) {
  const displayName = getExpertDisplayName(expert);
  const profession = getExpertProfession(expert);
  const description = getExpertDescription(expert);
  const avatarUrl = getExpertAvatarUrl(expert);
  const [avatarError, setAvatarError] = useState(false);
  const showAvatar = avatarUrl && !avatarError;

  return (
    <div
      role="button"
      tabIndex={0}
      onClick={() => onClick(expert)}
      onKeyDown={(e) => {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault();
          onClick(expert);
        }
      }}
      style={{
        display: 'flex',
        flexDirection: 'column',
        gap: 10,
        padding: 14,
        borderRadius: 14,
        border: '1px solid var(--color-border-secondary)',
        background: 'var(--color-bg-elevated)',
        cursor: 'pointer',
        transition: 'all 0.25s cubic-bezier(0.4, 0, 0.2, 1)',
        height: '100%',
        overflow: 'hidden',
      }}
      onMouseEnter={(e) => {
        (e.currentTarget as HTMLDivElement).style.transform = 'translateY(-4px)';
        (e.currentTarget as HTMLDivElement).style.borderColor = 'var(--color-primary)';
        (e.currentTarget as HTMLDivElement).style.boxShadow = 'var(--shadow-md)';
      }}
      onMouseLeave={(e) => {
        (e.currentTarget as HTMLDivElement).style.transform = 'translateY(0)';
        (e.currentTarget as HTMLDivElement).style.borderColor = 'var(--color-border-secondary)';
        (e.currentTarget as HTMLDivElement).style.boxShadow = 'none';
      }}
    >
      {/* 头部：头像 + 名称 + 职业 */}
      <div style={{ display: 'flex', alignItems: 'flex-start', gap: 10 }}>
        {showAvatar ? (
          <img
            src={avatarUrl}
            alt={displayName}
            onError={() => setAvatarError(true)}
            style={{
              width: 44,
              height: 44,
              borderRadius: 12,
              objectFit: 'cover',
              flexShrink: 0,
              border: '2px solid var(--color-border)',
            }}
          />
        ) : (
          <div style={{
            width: 44,
            height: 44,
            borderRadius: 12,
            background: 'linear-gradient(135deg, var(--color-info-bg-1) 0%, var(--color-primary-bg) 100%)',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            flexShrink: 0,
            border: '2px solid var(--color-border)',
          }}>
            <UserOutlined style={{ color: 'var(--color-info)', fontSize: 20 }} />
          </div>
        )}

        <div style={{ flex: 1, minWidth: 0 }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
            <Text strong style={{ fontSize: 14, color: 'var(--color-text)' }}>
              {displayName}
            </Text>
            <Tag color="blue" style={{ margin: 0, fontSize: 10, padding: '1px 6px' }}>
              专家
            </Tag>
          </div>
          {profession && (
            <div style={{ fontSize: 12, color: 'var(--color-text-secondary)', marginTop: 2 }}>
              {profession}
            </div>
          )}
        </div>
      </div>

      {/* 描述：最多 2 行 */}
      {description && (
        <div style={{
          fontSize: 12,
          color: 'var(--color-text-tertiary)',
          lineHeight: 1.4,
          display: '-webkit-box',
          WebkitLineClamp: 2,
          WebkitBoxOrient: 'vertical',
          overflow: 'hidden',
        }}>
          {description}
        </div>
      )}

      {/* 标签：分类 + 技能标签 */}
      <div style={{ display: 'flex', flexWrap: 'wrap', gap: 4 }}>
        {expert.category_id && (
          <Tag
            color="purple"
            style={{ margin: 0, fontSize: 10, padding: '2px 6px', borderRadius: 4 }}
          >
            {getCategoryName(expert.category_id)}
          </Tag>
        )}
        {expert.tags && expert.tags.slice(0, 3).map((tag, idx) => (
          <Tag
            key={idx}
            style={{
              margin: 0,
              fontSize: 10,
              padding: '2px 6px',
              borderRadius: 4,
              background: 'var(--color-bg-tertiary)',
              color: 'var(--color-text-secondary)',
              border: 'none',
            }}
          >
            {tag.zh || tag.en}
          </Tag>
        ))}
      </div>

      {/* 底部：技能数 + 查看详情箭头 */}
      <div style={{
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'space-between',
        marginTop: 'auto',
        paddingTop: 8,
        borderTop: '1px solid var(--color-border-light)',
      }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 4 }}>
          <ThunderboltOutlined style={{ fontSize: 12, color: 'var(--color-warning)' }} />
          <span style={{ fontSize: 12, color: 'var(--color-text-tertiary)' }}>
            {expert.skills.length} 项技能
          </span>
        </div>
        <RightOutlined style={{ fontSize: 14, color: 'var(--color-text-tertiary)' }} />
      </div>
    </div>
  );
}
