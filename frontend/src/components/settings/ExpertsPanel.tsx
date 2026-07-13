// 专家管理面板：Tab 分区展示专家和专家团，支持卡片点击弹出详情 Modal。
//
// 设计思路：
// - 使用 Ant Design Tabs 分开展示「专家」和「专家团队」，避免混在一起不方便查找
// - 卡片展示丰富信息：头像、名称、职业、标签、技能数、成员数（团队）等
// - 点击卡片弹出 Modal 大卡片，展示完整详情（描述、技能、成员列表、标签等）
// - 充分利用 plugin.json 中的信息：tags、skills、members、categoryId 等
// - 完整支持亮暗色主题，使用 CSS 变量确保主题适配

import { useState, useEffect, useMemo, useCallback, useRef } from 'react';
import { Button, Input, Empty, Spin, Modal, Dropdown, Tabs, Tag, Tooltip, Typography, App } from 'antd';
import type { MenuProps } from 'antd';
import {
  TeamOutlined,
  UserOutlined,
  ReloadOutlined,
  SearchOutlined,
  FileTextOutlined,
  ThunderboltOutlined,
  DownloadOutlined,
  UploadOutlined,
  FolderOpenOutlined,
  StarOutlined,
  RightOutlined,
} from '@ant-design/icons';
import { PageCard } from '@/components/common/PageCard';
import * as db from '@/utils/database';
import type { ExpertMetadata, SkillMetadata, ExpertMember } from '@/types/expert';
import {
  getExpertDisplayName,
  getExpertDescription,
  getExpertProfession,
  getExpertAvatarUrl,
} from '@/types/expert';

const { Paragraph, Text } = Typography;

/**
 * 获取分类名称映射
 *
 * 根据 plugin.json 中的 categoryId 字段，返回中文分类名称。
 * 分类 ID 格式如 "02-Engineering"、"08-FinanceInvestment"
 */
function getCategoryName(categoryId?: string): string {
  if (!categoryId) return '';
  const map: Record<string, string> = {
    '02-Engineering': '工程技术',
    '08-FinanceInvestment': '金融投资',
  };
  return map[categoryId] || categoryId.split('-').slice(1).join(' ');
}

/**
 * 单个专家卡片组件
 *
 * 展示头像、名称、职业、描述、标签、技能数等信息。
 * 卡片悬停时有上浮效果和边框高亮。
 */
function ExpertCard({ expert, onClick }: {
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

/**
 * 专家团队卡片组件
 *
 * 展示团队头像、名称、主理人、成员数、描述、标签等信息。
 * 团队卡片使用橙色系配色，与单个专家区分。
 */
function TeamCard({ expert, onClick }: {
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

      {/* 团队信息：主理人 + 成员数 */}
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
            主理人：{leadMember ? (leadMember.name_zh || leadMember.name_en || '未知') : '无'}
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

/**
 * 专家详情 Modal 组件
 *
 * 点击卡片后弹出的大卡片，展示完整的专家信息：
 * - 基本信息：头像、名称、职业、描述、版本、分类
 * - 标签列表
 * - 技能列表
 * - 团队成员列表（团队类型）
 * - Agent MD 内容
 *
 * 设计特点：
 * - 使用 Modal 替代 Drawer，更灵活自然
 * - 大卡片布局，信息层次分明
 * - 支持滚动查看长内容
 * - 暗色主题适配完善
 */
function ExpertDetailModal({
  open,
  expert,
  agentMd,
  skills,
  onClose,
  onExport,
  exporting,
}: {
  open: boolean;
  expert: ExpertMetadata | null;
  agentMd: string;
  skills: SkillMetadata[];
  onClose: () => void;
  onExport: () => void;
  exporting: boolean;
}) {
  if (!expert) return null;

  const displayName = getExpertDisplayName(expert);
  const profession = getExpertProfession(expert);
  const description = getExpertDescription(expert);
  const avatarUrl = getExpertAvatarUrl(expert);
  const isTeam = expert.expert_type === 'team';
  const [avatarError, setAvatarError] = useState(false);
  const showAvatar = avatarUrl && !avatarError;

  return (
    <Modal
      open={open}
      onCancel={onClose}
      footer={null}
      width={Math.min(720, typeof window !== 'undefined' ? window.innerWidth - 32 : 720)}
      centered
      style={{ borderRadius: 'var(--radius-lg)' }}
    >
      <div style={{
        maxHeight: '80vh',
        overflowY: 'auto',
        display: 'flex',
        flexDirection: 'column',
      }}>
        {/* 顶部背景区域 */}
        <div style={{
          padding: 24,
          background: isTeam
            ? 'linear-gradient(135deg, var(--color-warning-bg-1) 0%, var(--color-bg-elevated) 100%)'
            : 'linear-gradient(135deg, var(--color-info-bg-1) 0%, var(--color-bg-elevated) 100%)',
          borderBottom: '1px solid var(--color-border-light)',
        }}>
          {/* 头部：头像 + 名称 + 操作 */}
          <div style={{ display: 'flex', alignItems: 'flex-start', justifyContent: 'space-between', gap: 12, flexWrap: 'wrap' }}>
            <div style={{ display: 'flex', alignItems: 'flex-start', gap: 16, flex: 1, minWidth: 0 }}>
              {showAvatar ? (
                <img
                  src={avatarUrl}
                  alt={displayName}
                  onError={() => setAvatarError(true)}
                  style={{
                    width: 56,
                    height: 56,
                    borderRadius: 14,
                    objectFit: 'cover',
                    border: '3px solid var(--color-bg-elevated)',
                    boxShadow: 'var(--shadow-md)',
                    flexShrink: 0,
                  }}
                />
              ) : (
                <div style={{
                  width: 56,
                  height: 56,
                  borderRadius: 14,
                  background: isTeam
                    ? 'linear-gradient(135deg, var(--color-warning-bg-1), var(--color-error-bg))'
                    : 'linear-gradient(135deg, var(--color-info-bg-1), var(--color-primary-bg))',
                  display: 'flex',
                  alignItems: 'center',
                  justifyContent: 'center',
                  border: '3px solid var(--color-bg-elevated)',
                  boxShadow: 'var(--shadow-md)',
                  flexShrink: 0,
                }}>
                  {isTeam ? (
                    <TeamOutlined style={{ color: 'var(--color-warning)', fontSize: 26 }} />
                  ) : (
                    <UserOutlined style={{ color: 'var(--color-info)', fontSize: 26 }} />
                  )}
                </div>
              )}

              <div style={{ flex: 1, minWidth: 0 }}>
                <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 6, flexWrap: 'wrap' }}>
                  <Text strong style={{ fontSize: 18, color: 'var(--color-text)', wordBreak: 'break-word' }}>
                    {displayName}
                  </Text>
                  <Tag color={isTeam ? 'orange' : 'blue'} style={{ fontSize: 11, margin: 0 }}>
                    {isTeam ? '专家团队' : '单个专家'}
                  </Tag>
                  <Tag style={{ fontSize: 11, background: 'var(--color-bg-tertiary)', margin: 0 }}>
                    v{expert.version}
                  </Tag>
                </div>
                {profession && (
                  <div style={{ fontSize: 13, color: 'var(--color-text-secondary)', marginBottom: 8 }}>
                    {profession}
                  </div>
                )}
                {expert.category_id && (
                  <Tag color="purple" style={{ fontSize: 11 }}>
                    {getCategoryName(expert.category_id)}
                  </Tag>
                )}
              </div>
            </div>

            {/* 操作按钮 */}
            <div style={{ display: 'flex', gap: 8, flexShrink: 0 }}>
              <Tooltip title="导出为 zip 包">
                <Button
                  type="text"
                  icon={<DownloadOutlined />}
                  onClick={onExport}
                  loading={exporting}
                  style={{ color: 'var(--color-text-secondary)' }}
                >
                  导出
                </Button>
              </Tooltip>
            </div>
          </div>

          {/* 描述 */}
          {description && (
            <Paragraph
              type="secondary"
              style={{ marginTop: 16, marginBottom: 0, fontSize: 14, lineHeight: 1.6 }}
            >
              {description}
            </Paragraph>
          )}
        </div>

        {/* 内容区域 */}
        <div style={{ padding: 24, display: 'flex', flexDirection: 'column', gap: 20 }}>
          {/* 标签列表 */}
          {expert.tags && expert.tags.length > 0 && (
            <div>
              <div style={{
                fontWeight: 600,
                marginBottom: 10,
                display: 'flex',
                alignItems: 'center',
                gap: 6,
              }}>
                <StarOutlined style={{ fontSize: 14, color: 'var(--color-warning)' }} />
                标签
              </div>
              <div style={{ display: 'flex', flexWrap: 'wrap', gap: 6 }}>
                {expert.tags.map((tag, idx) => (
                  <Tag
                    key={idx}
                    style={{
                      fontSize: 12,
                      padding: '4px 10px',
                      borderRadius: 6,
                      background: 'var(--color-bg-tertiary)',
                      border: 'none',
                    }}
                  >
                    {tag.zh || tag.en}
                  </Tag>
                ))}
              </div>
            </div>
          )}

          {/* 团队成员列表（仅团队类型） */}
          {isTeam && expert.members && expert.members.length > 0 && (
            <div>
              <div style={{
                fontWeight: 600,
                marginBottom: 10,
                display: 'flex',
                alignItems: 'center',
                gap: 6,
              }}>
                <UserOutlined style={{ fontSize: 14, color: 'var(--color-warning)' }} />
                团队成员 ({expert.members.length})
              </div>
              <div style={{ display: 'grid', gridTemplateColumns: 'repeat(2, 1fr)', gap: 8 }}>
                {expert.members.map((member) => (
                  <MemberItem key={member.id} member={member} isLead={member.role === 'lead'} />
                ))}
              </div>
            </div>
          )}

          {/* 技能列表 */}
          <div>
            <div style={{
              fontWeight: 600,
              marginBottom: 10,
              display: 'flex',
              alignItems: 'center',
              gap: 6,
            }}>
              <ThunderboltOutlined style={{ fontSize: 14, color: 'var(--color-warning)' }} />
              关联技能 ({skills.length})
            </div>
            {skills.length === 0 ? (
              <Text type="secondary" style={{ fontSize: 13 }}>暂无关联技能</Text>
            ) : (
              <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
                {skills.map((skill) => (
                  <div
                    key={skill.skill_name}
                    style={{
                      padding: '12px 14px',
                      borderRadius: 10,
                      background: 'var(--color-bg-tertiary)',
                      border: '1px solid var(--color-border-light)',
                    }}
                  >
                    <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                      <span style={{ fontSize: 16 }}>{skill.yaml_emoji || '⚡'}</span>
                      <div style={{ flex: 1 }}>
                        <div style={{ fontWeight: 500, fontSize: 14, color: 'var(--color-text)' }}>
                          {skill.skill_name}
                        </div>
                        {(skill.yaml_description_zh || skill.yaml_description || skill.yaml_description_en) && (
                          <div style={{
                            fontSize: 12,
                            color: 'var(--color-text-secondary)',
                            marginTop: 4,
                            lineHeight: 1.5,
                          }}>
                            {skill.yaml_description_zh || skill.yaml_description || skill.yaml_description_en}
                          </div>
                        )}
                      </div>
                    </div>
                  </div>
                ))}
              </div>
            )}
          </div>

          {/* Agent MD 内容 */}
          <div>
            <div style={{
              fontWeight: 600,
              marginBottom: 10,
              display: 'flex',
              alignItems: 'center',
              gap: 6,
            }}>
              <FileTextOutlined style={{ fontSize: 14, color: 'var(--color-info)' }} />
              Agent MD
            </div>
            {agentMd ? (
              <pre style={{
                background: 'var(--color-bg)',
                color: 'var(--color-text-secondary)',
                padding: 14,
                borderRadius: 10,
                fontSize: 12,
                maxHeight: 300,
                overflow: 'auto',
                whiteSpace: 'pre-wrap',
                margin: 0,
                border: '1px solid var(--color-border)',
                lineHeight: 1.6,
              }}>
                {agentMd}
              </pre>
            ) : (
              <Text type="secondary" style={{ fontSize: 13 }}>暂无 Agent MD 内容</Text>
            )}
          </div>
        </div>
      </div>
    </Modal>
  );
}

/**
 * 团队成员项组件
 *
 * 展示单个成员的头像、名称、职业、角色。
 */
function MemberItem({ member, isLead }: { member: ExpertMember; isLead: boolean }) {
  return (
    <div style={{
      display: 'flex',
      alignItems: 'center',
      gap: 10,
      padding: 10,
      borderRadius: 10,
      background: isLead ? 'var(--color-warning-bg-1)' : 'var(--color-bg-tertiary)',
      border: '1px solid var(--color-border-light)',
    }}>
      {member.avatar_path ? (
        <img
          src={member.avatar_path}
          alt={member.name_zh || member.name_en || member.id}
          onError={() => {}}
          style={{
            width: 36,
            height: 36,
            borderRadius: '50%',
            objectFit: 'cover',
            flexShrink: 0,
          }}
        />
      ) : (
        <div style={{
          width: 36,
          height: 36,
          borderRadius: '50%',
          background: isLead ? 'var(--color-warning-bg-1)' : 'var(--color-info-bg-1)',
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center',
          flexShrink: 0,
        }}>
          {isLead ? (
            <StarOutlined style={{ color: 'var(--color-warning)', fontSize: 16 }} />
          ) : (
            <UserOutlined style={{ color: 'var(--color-info)', fontSize: 16 }} />
          )}
        </div>
      )}
      <div style={{ flex: 1, minWidth: 0 }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
          <Text style={{ fontSize: 13, fontWeight: isLead ? 600 : 500, color: 'var(--color-text)' }}>
            {member.name_zh || member.name_en || member.id}
          </Text>
          {isLead && (
            <Tag color="orange" style={{ margin: 0, fontSize: 9, padding: '0 4px' }}>
              主理人
            </Tag>
          )}
        </div>
        {(member.profession_zh || member.profession_en) && (
          <div style={{ fontSize: 11, color: 'var(--color-text-secondary)', marginTop: 2 }}>
            {member.profession_zh || member.profession_en}
          </div>
        )}
      </div>
    </div>
  );
}

/**
 * 专家管理面板入口
 *
 * 功能：
 * - Tabs 分开展示「专家」和「专家团队」
 * - 关键字搜索过滤（跨两个 Tab）
 * - 点击卡片弹出 Modal 大卡片查看详情
 * - 支持导入（zip、WorkBuddy、目录）和导出功能
 * - 重新加载专家定义目录
 */
export function ExpertsPanel() {
  const { message } = App.useApp();
  const [experts, setExperts] = useState<ExpertMetadata[]>([]);
  const [loading, setLoading] = useState(false);
  const [reloading, setReloading] = useState(false);
  const [searchText, setSearchText] = useState('');
  // 详情 Modal 相关状态
  const [selectedExpert, setSelectedExpert] = useState<ExpertMetadata | null>(null);
  const [detailOpen, setDetailOpen] = useState(false);
  const [agentMd, setAgentMd] = useState('');
  const [skills, setSkills] = useState<SkillMetadata[]>([]);
  const [exporting, setExporting] = useState(false);
  // 导入相关状态
  const fileInputRef = useRef<HTMLInputElement>(null);
  const [importing, setImporting] = useState(false);
  const [dirImportModalOpen, setDirImportModalOpen] = useState(false);
  const [dirImportPath, setDirImportPath] = useState('');
  const [workbuddyImporting, setWorkbuddyImporting] = useState(false);

  // 加载专家列表
  const loadExperts = useCallback(async () => {
    setLoading(true);
    try {
      const data = await db.getAllExperts();
      setExperts(data);
    } catch (err: any) {
      message.error('加载专家列表失败: ' + (err?.message || String(err)));
      setExperts([]);
    } finally {
      setLoading(false);
    }
  }, [message]);

  // 初始加载
  useEffect(() => {
    loadExperts();
  }, [loadExperts]);

  // 重新加载
  const handleReload = useCallback(async () => {
    setReloading(true);
    try {
      const result = await db.reloadExperts();
      if (result.errors.length > 0) {
        message.warning(`加载完成：${result.loaded_count} 个成功，${result.errors.length} 个失败`);
      } else {
        message.success(`已重新加载 ${result.loaded_count} 个专家`);
      }
      await loadExperts();
    } catch (err: any) {
      message.error('重新加载失败: ' + (err?.message || String(err)));
    } finally {
      setReloading(false);
    }
  }, [loadExperts, message]);

  // 打开详情 Modal
  const handleOpenDetail = useCallback(async (expert: ExpertMetadata) => {
    setSelectedExpert(expert);
    setDetailOpen(true);
    setAgentMd('');
    setSkills([]);
    // 异步加载详情数据
    try {
      const [mdContent, skillList] = await Promise.all([
        db.getExpertAgentMd(expert.name).catch(() => ''),
        db.getExpertSkills(expert.name).catch(() => [] as SkillMetadata[]),
      ]);
      setAgentMd(mdContent);
      setSkills(skillList);
    } catch {
      // 加载失败不阻塞展示
    }
  }, []);

  // 关闭详情 Modal
  const handleCloseDetail = useCallback(() => {
    setDetailOpen(false);
    setSelectedExpert(null);
    setAgentMd('');
    setSkills([]);
  }, []);

  // 导出专家
  const handleExport = useCallback(async () => {
    if (!selectedExpert) return;
    setExporting(true);
    try {
      const blob = await db.exportExpert(selectedExpert.name);
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = `${selectedExpert.name}.zip`;
      document.body.appendChild(a);
      a.click();
      document.body.removeChild(a);
      URL.revokeObjectURL(url);
      message.success('导出成功');
    } catch (err: any) {
      message.error('导出失败: ' + (err?.message || String(err)));
    } finally {
      setExporting(false);
    }
  }, [selectedExpert, message]);

  // 文件上传导入
  const handleFileImport = useCallback(async (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    e.target.value = '';
    if (!file) return;
    setImporting(true);
    try {
      const result = await db.importExpert(file);
      if (result.errors.length > 0) {
        message.warning(`导入完成：${result.errors.length} 条警告`);
      } else {
        message.success('导入成功');
      }
      await loadExperts();
    } catch (err: any) {
      message.error('导入失败: ' + (err?.message || String(err)));
    } finally {
      setImporting(false);
    }
  }, [loadExperts, message]);

  // 目录导入
  const handleDirImport = useCallback(async () => {
    if (!dirImportPath.trim()) {
      message.warning('请输入目录路径');
      return;
    }
    setImporting(true);
    try {
      const result = await db.importExpertFromDirectory(dirImportPath.trim());
      if (result.errors.length > 0) {
        message.warning(`导入完成：${result.errors.length} 条警告`);
      } else {
        message.success('导入成功');
      }
      setDirImportModalOpen(false);
      setDirImportPath('');
      await loadExperts();
    } catch (err: any) {
      message.error('导入失败: ' + (err?.message || String(err)));
    } finally {
      setImporting(false);
    }
  }, [dirImportPath, loadExperts, message]);

  // WorkBuddy 批量导入
  const handleWorkbuddyImport = useCallback(async () => {
    setWorkbuddyImporting(true);
    try {
      const result = await db.importFromWorkbuddy();
      const parts: string[] = [];
      if (result.imported_count > 0) parts.push(`新增 ${result.imported_count} 个`);
      if (result.skipped_count > 0) parts.push(`跳过 ${result.skipped_count} 个（已存在）`);
      if (result.errors.length > 0) parts.push(`${result.errors.length} 个失败`);

      if (parts.length === 0) {
        message.info('WorkBuddy 目录中未找到可导入的专家');
      } else if (result.errors.length > 0) {
        message.warning(`WorkBuddy 导入：${parts.join('，')}`);
      } else {
        message.success(`WorkBuddy 导入：${parts.join('，')}`);
      }
      await loadExperts();
    } catch (err: any) {
      message.error('WorkBuddy 导入失败: ' + (err?.message || String(err)));
    } finally {
      setWorkbuddyImporting(false);
    }
  }, [loadExperts, message]);

  // 搜索过滤
  const filteredExperts = useMemo(() => {
    if (!searchText.trim()) return experts;
    const keyword = searchText.trim().toLowerCase();
    return experts.filter(expert => {
      const name = getExpertDisplayName(expert).toLowerCase();
      const profession = getExpertProfession(expert).toLowerCase();
      const description = getExpertDescription(expert).toLowerCase();
      return name.includes(keyword) || profession.includes(keyword) || description.includes(keyword);
    });
  }, [experts, searchText]);

  // 按类型分组
  const individualExperts = useMemo(
    () => filteredExperts.filter(e => e.expert_type === 'agent'),
    [filteredExperts],
  );
  const teamExperts = useMemo(
    () => filteredExperts.filter(e => e.expert_type === 'team'),
    [filteredExperts],
  );

  // 导入下拉菜单
  const importMenuItems: MenuProps['items'] = [
    {
      key: 'zip',
      label: '上传 zip 包',
      icon: <UploadOutlined />,
      onClick: () => fileInputRef.current?.click(),
    },
    {
      key: 'workbuddy',
      label: '从 WorkBuddy 导入',
      icon: <TeamOutlined />,
      onClick: handleWorkbuddyImport,
      disabled: workbuddyImporting,
    },
    {
      key: 'dir',
      label: '从本地目录导入',
      icon: <FolderOpenOutlined />,
      onClick: () => setDirImportModalOpen(true),
    },
  ];

  return (
    <PageCard
      icon={<TeamOutlined />}
      title="专家"
      extra={
        <div style={{ display: 'flex', gap: 4 }}>
          <Dropdown menu={{ items: importMenuItems }} placement="bottomRight">
            <Tooltip title="导入专家">
              <Button
                type="text"
                icon={<UploadOutlined />}
                loading={importing}
              >
                导入
              </Button>
            </Tooltip>
          </Dropdown>
          <Tooltip title="重新扫描 ~/.ntd/experts/ 目录">
            <Button
              type="text"
              icon={<ReloadOutlined spin={reloading} />}
              onClick={handleReload}
              disabled={reloading}
            >
              重新加载
            </Button>
          </Tooltip>
        </div>
      }
    >
      {/* 搜索栏 */}
      <div style={{ display: 'flex', alignItems: 'center', gap: 12, marginBottom: 16, flexWrap: 'wrap' }}>
        <Input
          allowClear
          size="small"
          placeholder="搜索专家名称、职业或描述"
          prefix={<SearchOutlined />}
          value={searchText}
          onChange={(e) => setSearchText(e.target.value)}
          style={{ width: 280 }}
        />
        <span style={{ color: 'var(--color-text-secondary)', fontSize: 12 }}>
          共 {filteredExperts.length} 个（专家 {individualExperts.length} / 团队 {teamExperts.length}）
        </span>
      </div>

      {/* Tabs 分区 */}
      <Tabs
        defaultActiveKey="experts"
        items={[
          {
            key: 'experts',
            label: (
              <span style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
                <UserOutlined />
                专家 ({individualExperts.length})
              </span>
            ),
            children: (
              <Spin spinning={loading}>
                {individualExperts.length === 0 && !loading ? (
                  <Empty
                    description="暂无专家"
                    image={Empty.PRESENTED_IMAGE_SIMPLE}
                    style={{ padding: '48px 0' }}
                  />
                ) : (
                  <div style={{
                    display: 'grid',
                    gridTemplateColumns: 'repeat(auto-fill, minmax(280px, 1fr))',
                    gap: 14,
                  }}>
                    {individualExperts.map((expert) => (
                      <ExpertCard
                        key={expert.name}
                        expert={expert}
                        onClick={handleOpenDetail}
                      />
                    ))}
                  </div>
                )}
              </Spin>
            ),
          },
          {
            key: 'teams',
            label: (
              <span style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
                <TeamOutlined />
                专家团队 ({teamExperts.length})
              </span>
            ),
            children: (
              <Spin spinning={loading}>
                {teamExperts.length === 0 && !loading ? (
                  <Empty
                    description="暂无专家团队"
                    image={Empty.PRESENTED_IMAGE_SIMPLE}
                    style={{ padding: '48px 0' }}
                  />
                ) : (
                  <div style={{
                    display: 'grid',
                    gridTemplateColumns: 'repeat(auto-fill, minmax(280px, 1fr))',
                    gap: 14,
                  }}>
                    {teamExperts.map((expert) => (
                      <TeamCard
                        key={expert.name}
                        expert={expert}
                        onClick={handleOpenDetail}
                      />
                    ))}
                  </div>
                )}
              </Spin>
            ),
          },
        ]}
      />

      {/* 详情 Modal */}
      <ExpertDetailModal
        open={detailOpen}
        expert={selectedExpert}
        agentMd={agentMd}
        skills={skills}
        onClose={handleCloseDetail}
        onExport={handleExport}
        exporting={exporting}
      />

      {/* 隐藏的文件上传 */}
      <input
        ref={fileInputRef}
        type="file"
        accept=".zip"
        style={{ display: 'none' }}
        onChange={handleFileImport}
      />

      {/* 目录导入弹窗 */}
      <Modal
        title="从本地目录导入专家"
        open={dirImportModalOpen}
        onOk={handleDirImport}
        onCancel={() => { setDirImportModalOpen(false); setDirImportPath(''); }}
        okText="导入"
        cancelText="取消"
        confirmLoading={importing}
      >
        <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
          <div style={{ fontSize: 13, color: 'var(--color-text-secondary)' }}>
            请输入专家定义目录的绝对路径，例如 WorkBuddy 插件目录。
            <br />
            目录下需包含 <code>.codebuddy-plugin/plugin.json</code> 文件。
          </div>
          <Input
            placeholder="~/.workbuddy/plugins/marketplaces/experts/plugins/senior-developer"
            value={dirImportPath}
            onChange={(e) => setDirImportPath(e.target.value)}
          />
        </div>
      </Modal>
    </PageCard>
  );
}
