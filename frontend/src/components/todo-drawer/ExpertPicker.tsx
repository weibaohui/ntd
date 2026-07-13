import { memo, useState, useEffect } from 'react';
import { CheckOutlined, UserOutlined, TeamOutlined, ReloadOutlined } from '@ant-design/icons';
import { App, Empty, Spin, Tooltip } from 'antd';
import * as db from '@/utils/database';
import type { ExpertMetadata } from '@/types/expert';
import { getExpertDisplayName, getExpertDescription, getExpertProfession } from '@/types/expert';

/**
 * 专家选择器组件
 *
 * 展示所有可用的专家和专家团队，用户可以选择其中一个关联到 Todo。
 * 支持单个专家和专家团队两种类型，分别用不同的图标标识。
 */
export const ExpertPicker = memo(function ExpertPicker({
  value,
  onChange,
}: {
  /** 当前选中的专家名称，null/undefined 表示未选择 */
  value?: string | null;
  /** 选择变化回调 */
  onChange: (expertName: string | null) => void;
}) {
  const { message } = App.useApp();
  const [experts, setExperts] = useState<ExpertMetadata[]>([]);
  const [loading, setLoading] = useState(false);
  const [reloading, setReloading] = useState(false);

  // 加载专家列表
  const loadExperts = async () => {
    setLoading(true);
    try {
      const data = await db.getAllExperts();
      setExperts(data);
    } catch {
      // 加载失败时静默处理，显示空状态
      setExperts([]);
    } finally {
      setLoading(false);
    }
  };

  // 初始加载
  useEffect(() => {
    loadExperts();
  }, []);

  // 重新加载专家定义
  const handleReload = async () => {
    setReloading(true);
    try {
      const result = await db.reloadExperts();
      if (result.errors.length > 0) {
        message.warning(`加载完成，${result.loaded_count} 个成功，${result.errors.length} 个失败`);
      } else {
        message.success(`已重新加载 ${result.loaded_count} 个专家`);
      }
      await loadExperts();
    } catch (err: any) {
      message.error('重新加载失败: ' + (err?.message || String(err)));
    } finally {
      setReloading(false);
    }
  };

  // 切换选择
  const handleSelect = (expertName: string) => {
    if (value === expertName) {
      // 再次点击已选中的专家则取消选择
      onChange(null);
    } else {
      onChange(expertName);
    }
  };

  // 按类型分组：团队在前，专家在后
  const sortedExperts = [...experts].sort((a, b) => {
    if (a.expert_type === b.expert_type) return 0;
    return a.expert_type === 'team' ? -1 : 1;
  });

  return (
    <div style={{ marginBottom: 16 }}>
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 10 }}>
        <div style={{ fontWeight: 600, fontSize: 14 }}>专家/团队</div>
        <Tooltip title="重新加载专家定义">
          <ReloadOutlined
            style={{
              fontSize: 14,
              color: 'var(--color-text-secondary)',
              cursor: reloading ? 'not-allowed' : 'pointer',
              opacity: reloading ? 0.5 : 1,
            }}
            onClick={reloading ? undefined : handleReload}
            spin={reloading}
          />
        </Tooltip>
      </div>

      {loading ? (
        <div style={{ display: 'flex', justifyContent: 'center', padding: '24px 0' }}>
          <Spin size="small" />
        </div>
      ) : sortedExperts.length === 0 ? (
        <Empty
          description="暂无专家"
          image={Empty.PRESENTED_IMAGE_SIMPLE}
          style={{ padding: '16px 0' }}
        />
      ) : (
        <div style={{ display: 'flex', flexWrap: 'wrap', gap: 10 }}>
          {sortedExperts.map((expert) => {
            const selected = value === expert.name;
            const isTeam = expert.expert_type === 'team';
            const displayName = getExpertDisplayName(expert);
            const profession = getExpertProfession(expert);
            const description = getExpertDescription(expert);

            return (
              <div
                key={expert.name}
                onClick={() => handleSelect(expert.name)}
                role="button"
                tabIndex={0}
                onKeyDown={(e) => {
                  if (e.key === 'Enter' || e.key === ' ') {
                    e.preventDefault();
                    handleSelect(expert.name);
                  }
                }}
                style={{
                  display: 'flex',
                  alignItems: 'flex-start',
                  gap: 10,
                  padding: '10px 12px',
                  borderRadius: 10,
                  border: `2px solid ${selected ? 'var(--color-primary)' : 'var(--color-border-secondary)'}`,
                  background: selected ? 'var(--color-primary-bg-1)' : 'var(--color-bg-elevated)',
                  cursor: 'pointer',
                  transition: 'all 0.2s ease',
                  flex: '1 1 calc(50% - 10px)',
                  minWidth: 180,
                }}
                onMouseEnter={(e) => {
                  if (!selected) {
                    (e.currentTarget as HTMLDivElement).style.borderColor = 'var(--color-primary-hover)';
                    (e.currentTarget as HTMLDivElement).style.background = 'var(--color-primary-bg-2)';
                  }
                }}
                onMouseLeave={(e) => {
                  if (!selected) {
                    (e.currentTarget as HTMLDivElement).style.borderColor = 'var(--color-border-secondary)';
                    (e.currentTarget as HTMLDivElement).style.background = 'var(--color-bg-elevated)';
                  }
                }}
              >
                {/* 类型图标 */}
                <div
                  style={{
                    width: 32,
                    height: 32,
                    borderRadius: 8,
                    background: isTeam ? 'var(--color-warning-bg-1)' : 'var(--color-info-bg-1)',
                    display: 'flex',
                    alignItems: 'center',
                    justifyContent: 'center',
                    flexShrink: 0,
                  }}
                >
                  {isTeam ? (
                    <TeamOutlined style={{ color: 'var(--color-warning)', fontSize: 16 }} />
                  ) : (
                    <UserOutlined style={{ color: 'var(--color-info)', fontSize: 16 }} />
                  )}
                </div>

                {/* 专家信息 */}
                <div style={{ flex: 1, minWidth: 0 }}>
                  <div
                    style={{
                      fontSize: 14,
                      fontWeight: 600,
                      color: selected ? 'var(--color-primary)' : 'var(--color-text)',
                      display: 'flex',
                      alignItems: 'center',
                      gap: 6,
                    }}
                  >
                    <span style={{ overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                      {displayName}
                    </span>
                    {selected && (
                      <CheckOutlined style={{ fontSize: 12, color: 'var(--color-primary)' }} />
                    )}
                  </div>
                  {profession && (
                    <div
                      style={{
                        fontSize: 12,
                        color: 'var(--color-text-secondary)',
                        marginTop: 2,
                        overflow: 'hidden',
                        textOverflow: 'ellipsis',
                        whiteSpace: 'nowrap',
                      }}
                    >
                      {profession}
                    </div>
                  )}
                  {description && (
                    <div
                      style={{
                        fontSize: 11,
                        color: 'var(--color-text-tertiary)',
                        marginTop: 4,
                        display: '-webkit-box',
                        WebkitLineClamp: 2,
                        WebkitBoxOrient: 'vertical',
                        overflow: 'hidden',
                        lineHeight: 1.4,
                      }}
                    >
                      {description}
                    </div>
                  )}
                </div>

                {/* 团队标识 */}
                {isTeam && (
                  <div
                    style={{
                      fontSize: 10,
                      padding: '2px 6px',
                      borderRadius: 4,
                      background: 'var(--color-warning-bg-1)',
                      color: 'var(--color-warning)',
                      flexShrink: 0,
                    }}
                  >
                    团队
                  </div>
                )}
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
});
