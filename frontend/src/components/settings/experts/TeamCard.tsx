// 专家团队卡片：展示团队头像、名称、负责人、成员数、描述、分类，悬停上浮。
// 从 ExpertsPanel 拆出；橙色系，与 ExpertCard（蓝色系）区分类型。

import { useState } from 'react';
import { Tag, Typography } from 'antd';
import {
  TeamOutlined,
  UserOutlined,
  StarOutlined,
  ThunderboltOutlined,
  RightOutlined,
} from '@ant-design/icons';
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
 * 专家团队卡片组件
 *
 * 展示团队头像、名称、负责人、成员数、描述、标签等信息。
 * 团队卡片使用橙色系配色，与单个专家区分。
 */
export function TeamCard({ expert, onClick }: {
  expert: ExpertMetadata;
  onClick: (expert: ExpertMetadata) => void;
}) {
  const displayName = getExpertDisplayName(expert);
  const profession = getExpertProfession(expert);
  const description = getExpertDescription(expert);
  const avatarUrl = getExpertAvatarUrl(expert);
  const [avatarError, setAvatarError] = useState(false);
  const showAvatar = avatarUrl && !avatarError;
  const memberCount = expert.members?.length || 0;
  const leadMember = expert.members?.find(m => m.role === 'lead');

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
        (e.currentTarget as HTMLDivElement).style.borderColor = 'var(--color-warning)';
        (e.currentTarget as HTMLDivElement).style.boxShadow = 'var(--shadow-md)';
      }}
      onMouseLeave={(e) => {
        (e.currentTarget as HTMLDivElement).style.transform = 'translateY(0)';
        (e.currentTarget as HTMLDivElement).style.borderColor = 'var(--color-border-secondary)';
        (e.currentTarget as HTMLDivElement).style.boxShadow = 'none';
      }}
    >
      {/* 头部：团队头像 + 名称 + 类型标签 */}
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
            background: 'linear-gradient(135deg, var(--color-warning-bg-1) 0%, var(--color-error-bg) 100%)',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            flexShrink: 0,
            border: '2px solid var(--color-border)',
          }}>
            <TeamOutlined style={{ color: 'var(--color-warning)', fontSize: 20 }} />
          </div>
        )}

        <div style={{ flex: 1, minWidth: 0 }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
            <Text strong style={{ fontSize: 14, color: 'var(--color-text)' }}>
              {displayName}
            </Text>
            <Tag color="orange" style={{ margin: 0, fontSize: 10, padding: '1px 6px' }}>
              团队
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

      {/* 团队信息：负责人 + 成员数 */}
      <div style={{
        display: 'flex',
        alignItems: 'center',
        gap: 8,
        padding: 8,
        borderRadius: 8,
        background: 'var(--color-warning-bg-1)',
      }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 4 }}>
          <StarOutlined style={{ fontSize: 12, color: 'var(--color-warning)' }} />
          <span style={{ fontSize: 11, color: 'var(--color-text-secondary)' }}>
            负责人：{leadMember ? (leadMember.name_zh || leadMember.name_en || '未知') : '无'}
          </span>
        </div>
        <div style={{ width: 1, height: 12, background: 'var(--color-border)' }} />
        <div style={{ display: 'flex', alignItems: 'center', gap: 4 }}>
            <UserOutlined style={{ fontSize: 12, color: 'var(--color-warning)' }} />
            <span style={{ fontSize: 11, color: 'var(--color-text-secondary)' }}>
              {memberCount} 人团队
            </span>
          </div>
      </div>

      {/* 标签：分类标签 */}
      {expert.category_id && (
        <div style={{ display: 'flex', flexWrap: 'wrap', gap: 4 }}>
          <Tag
            color="purple"
            style={{ margin: 0, fontSize: 10, padding: '2px 6px', borderRadius: 4 }}
          >
            {getCategoryName(expert.category_id)}
          </Tag>
        </div>
      )}

      {/* 底部：查看详情箭头 */}
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
