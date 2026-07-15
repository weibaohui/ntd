// 专家详情 Modal：点击卡片弹出的大卡片，展示完整信息（描述、标签、技能、成员、Agent MD）。
// 含团队成员项 MemberItem（仅本组件使用，故同文件不单独导出）。
// 从 ExpertsPanel 拆出。

import { useState } from 'react';
// 复用既有断点 hook(阈值 768px)，避免在本组件重复实现手机端判定。
import { useIsMobile } from '@/hooks/useIsMobile';
import { Badge, Button, Modal, Tag, Tooltip, Typography } from 'antd';
import {
  TeamOutlined,
  UserOutlined,
  StarOutlined,
  ThunderboltOutlined,
  FileTextOutlined,
  DownloadOutlined,
  DeleteOutlined,
} from '@ant-design/icons';
import type { ExpertMetadata, SkillMetadata, ExpertMember } from '@/types/expert';
import {
  getExpertDisplayName,
  getExpertProfession,
  getExpertDescription,
  getExpertAvatarUrl,
  getMemberAvatarUrl,
  getCategoryName,
} from '@/types/expert';

const { Paragraph, Text } = Typography;

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
export function ExpertDetailModal({
  open,
  expert,
  agentMd,
  skills,
  onClose,
  onExport,
  exporting,
  onDelete,
  deleting,
}: {
  open: boolean;
  expert: ExpertMetadata | null;
  agentMd: string;
  skills: SkillMetadata[];
  onClose: () => void;
  onExport: () => void;
  exporting: boolean;
  onDelete: () => void;
  deleting: boolean;
}) {
  // Hooks 必须在任何条件 return 之前调用：否则 expert 从 null→对象时 Hook 数量
  // 从 0 变 2，违反 Hooks 顺序规则，首次打开详情会崩溃。
  const [avatarError, setAvatarError] = useState(false);
  const [deleteConfirmOpen, setDeleteConfirmOpen] = useState(false);
  // 手机端判定：决定 header 是横排(桌面)还是纵向堆叠(手机)。
  // 必须在下面的条件 return 之前调用，否则 expert 从 null→对象时 Hook 数量变化会崩溃。
  const isMobile = useIsMobile();
  if (!expert) return null;

  const displayName = getExpertDisplayName(expert);
  const profession = getExpertProfession(expert);
  const description = getExpertDescription(expert);
  const avatarUrl = getExpertAvatarUrl(expert);
  const isTeam = expert.expert_type === 'team';
  const showAvatar = avatarUrl && !avatarError;

  // 打开删除确认
  const handleDeleteClick = () => {
    setDeleteConfirmOpen(true);
  };

  // 确认删除
  const handleConfirmDelete = () => {
    setDeleteConfirmOpen(false);
    onDelete();
  };

  return (
    <>
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
          {/* 头部：头像 + 名称 + 操作。
              桌面横排(头像文字在左、按钮在右)；手机端纵向堆叠，让操作按钮整行下移，
              避免与文字列抢宽度、把 profession 挤到几像素宽而逐字换行成竖排。 */}
          <div style={{
            display: 'flex',
            // 桌面头像顶部对齐；手机端纵向时改 stretch，让左侧文字块水平占满 modal 宽度，
            // 否则 flex-start 会让文字块只取内容宽，文字列仍被压缩、profession 重新变窄。
            alignItems: isMobile ? 'stretch' : 'flex-start',
            // 手机端改纵向：主轴变垂直，操作按钮自然落到文字块下方。
            flexDirection: isMobile ? 'column' : 'row',
            // 桌面两端对齐；手机端纵向时改左对齐——space-between 在内容自适应高度下会撑出多余垂直间距。
            justifyContent: isMobile ? 'flex-start' : 'space-between',
            gap: 12,
            flexWrap: 'wrap',
          }}>
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
                  // profession 可能是较长串(如「Git工作流专家」)，加 wordBreak/overflowWrap
                  // 防止极端窄宽下逐字换行成竖排；正常宽度下无副作用，双保险。
                  // data-testid 供 Playwright 定位，断言手机端 profession 横向不竖排。
                  <div
                    data-testid="expert-profession"
                    style={{
                      fontSize: 13,
                      color: 'var(--color-text-secondary)',
                      marginBottom: 8,
                      wordBreak: 'break-word',
                      overflowWrap: 'break-word',
                    }}>
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

            {/* 操作按钮：flexShrink:0 保证不被压缩。
                手机端 header 改纵向后，用 alignSelf 贴右，与上方文字块在视觉上右对齐。 */}
            <div style={{
              display: 'flex',
              gap: 8,
              flexShrink: 0,
              alignSelf: isMobile ? 'flex-end' : 'auto',
            }}>
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
              <Tooltip title="删除专家">
                <Button
                  type="text"
                  icon={<DeleteOutlined />}
                  onClick={handleDeleteClick}
                  loading={deleting}
                  style={{ color: 'var(--color-text-secondary)' }}
                >
                  删除
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
                  <MemberItem
                    key={member.id}
                    member={member}
                    isLead={member.role === 'lead'}
                    expertName={expert.name}
                  />
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

    {/* 删除确认对话框 */}
    <Modal
      open={deleteConfirmOpen}
      onCancel={() => setDeleteConfirmOpen(false)}
      footer={null}
      width={Math.min(480, typeof window !== 'undefined' ? window.innerWidth - 32 : 480)}
      centered
    >
      <div style={{ padding: '20px 0' }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 12, marginBottom: 16 }}>
          <div style={{
            width: 48,
            height: 48,
            borderRadius: '50%',
            background: 'var(--color-error-bg-1)',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
          }}>
            <DeleteOutlined style={{ color: 'var(--color-error)', fontSize: 24 }} />
          </div>
          <div>
            <Text strong style={{ fontSize: 16, color: 'var(--color-text)' }}>
              确认删除
            </Text>
          </div>
        </div>
        <Paragraph style={{ fontSize: 14, lineHeight: 1.6, marginBottom: 20 }}>
          确定要删除「{displayName}」吗？此操作将永久删除该专家及其所有关联数据，<br />
          <Text strong style={{ color: 'var(--color-error)' }}>删除后无法恢复。</Text>
        </Paragraph>
        <div style={{ display: 'flex', gap: 10, justifyContent: 'flex-end' }}>
          <Button
            onClick={() => setDeleteConfirmOpen(false)}
            style={{ minWidth: 80 }}
          >
            取消
          </Button>
          <Button
            type="primary"
            danger
            onClick={handleConfirmDelete}
            loading={deleting}
            style={{ minWidth: 80 }}
          >
            确认删除
          </Button>
        </div>
      </div>
    </Modal>
    </>
  );
}

/**
 * 团队成员项组件
 *
 * 展示单个成员的头像、名称、职业、角色。
 * 负责人使用 Badge.Ribbon 右上角绶带标记，避免在窄屏挤压名称导致布局变形。
 *
 * 头像加载：成员的 avatar_path 是相对路径，需通过后端接口获取二进制。
 * 头像加载失败时回退到默认图标，保证布局不塌陷。
 */
function MemberItem({
  member,
  isLead,
  expertName,
}: {
  member: ExpertMember;
  isLead: boolean;
  expertName: string;
}) {
  // 头像加载失败状态：true 表示走兜底图标
  const [avatarError, setAvatarError] = useState(false);
  // 仅当成员配置了 avatar_path 且未发生加载失败时才尝试展示图片
  const showAvatar = !!member.avatar_path && !avatarError;

  // 成员卡片内容：头像 + 名称 + 职业
  const cardContent = (
    <div style={{
      display: 'flex',
      alignItems: 'center',
      gap: 10,
      padding: 10,
      borderRadius: 10,
      background: isLead ? 'var(--color-warning-bg-1)' : 'var(--color-bg-tertiary)',
      border: '1px solid var(--color-border-light)',
      // Ribbon 需要父元素 overflow hidden 才能裁切绶带折角
      overflow: 'hidden',
    }}>
      {showAvatar ? (
        <img
          src={getMemberAvatarUrl(expertName, member.id)}
          alt={member.name_zh || member.name_en || member.id}
          // 头像加载失败时切换到兜底图标，避免出现破图
          onError={() => setAvatarError(true)}
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
        <Text style={{ fontSize: 13, fontWeight: isLead ? 600 : 500, color: 'var(--color-text)' }}>
          {member.name_zh || member.name_en || member.id}
        </Text>
        {(member.profession_zh || member.profession_en) && (
          <div style={{ fontSize: 11, color: 'var(--color-text-secondary)', marginTop: 2 }}>
            {member.profession_zh || member.profession_en}
          </div>
        )}
      </div>
    </div>
  );

  // 负责人用 Badge.Ribbon 右上角绶带标记，非负责人直接返回卡片
  if (isLead) {
    return (
      <Badge.Ribbon text="负责人" color="orange">
        {cardContent}
      </Badge.Ribbon>
    );
  }
  return cardContent;
}
