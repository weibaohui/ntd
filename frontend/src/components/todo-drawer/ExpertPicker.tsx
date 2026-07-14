import { memo, useState, useEffect, useMemo, useCallback } from 'react';
import {
  CheckOutlined,
  UserOutlined,
  TeamOutlined,
  ReloadOutlined,
  CloseOutlined,
  SearchOutlined,
} from '@ant-design/icons';
import { App, Empty, Input, Modal, Spin, Tooltip } from 'antd';
import * as db from '@/utils/database';
import type { ExpertMetadata } from '@/types/expert';
import { getExpertDisplayName, getExpertDescription, getExpertProfession } from '@/types/expert';

/**
 * 专家选择器组件（弹窗模式）
 *
 * 点击触发按钮打开 Modal，在 Modal 中搜索和点选专家/专家团队。
 * 适用于大量专家场景，避免铺开式布局占用过多空间。
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
  // 专家列表及加载状态
  const [experts, setExperts] = useState<ExpertMetadata[]>([]);
  const [loading, setLoading] = useState(false);
  const [reloading, setReloading] = useState(false);
  // 弹窗控制
  const [open, setOpen] = useState(false);
  // 搜索关键词
  const [keyword, setKeyword] = useState('');

  // 加载专家列表
  const loadExperts = useCallback(async () => {
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
  }, []);

  // 初始加载
  useEffect(() => {
    loadExperts();
  }, [loadExperts]);

  // 重新加载专家定义（弹窗内刷新按钮触发）
  const handleReload = useCallback(async () => {
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
  }, [loadExperts, message]);

  // 选中专家：选中后关闭弹窗
  const handleSelect = useCallback((expertName: string) => {
    onChange(expertName);
    setOpen(false);
    setKeyword('');
  }, [onChange]);

  // 清除选择
  const handleClear = useCallback(() => {
    onChange(null);
  }, [onChange]);

  // 按类型排序：团队在前，专家在后；同类型按名称排序
  const sortedExperts = useMemo(() => {
    return [...experts].sort((a, b) => {
      if (a.expert_type !== b.expert_type) {
        return a.expert_type === 'team' ? -1 : 1;
      }
      // 同类型按展示名称排序
      return getExpertDisplayName(a).localeCompare(getExpertDisplayName(b), 'zh');
    });
  }, [experts]);

  // 搜索过滤：匹配名称、职业、描述
  const filteredExperts = useMemo(() => {
    if (!keyword.trim()) return sortedExperts;
    const kw = keyword.trim().toLowerCase();
    return sortedExperts.filter((e) => {
      const name = getExpertDisplayName(e).toLowerCase();
      const profession = getExpertProfession(e).toLowerCase();
      const description = getExpertDescription(e).toLowerCase();
      return name.includes(kw) || profession.includes(kw) || description.includes(kw);
    });
  }, [sortedExperts, keyword]);

  // 当前选中的专家元数据，用于触发按钮展示
  const selectedExpert = useMemo(() => {
    if (!value) return null;
    return experts.find((e) => e.name === value) ?? null;
  }, [experts, value]);

  return (
    <div>
      {/* 触发按钮：显示当前选中的专家或"选择专家" */}
      <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
        <div
          onClick={() => setOpen(true)}
          role="button"
          tabIndex={0}
          onKeyDown={(e) => {
            if (e.key === 'Enter' || e.key === ' ') {
              e.preventDefault();
              setOpen(true);
            }
          }}
          style={{
            flex: 1,
            display: 'flex',
            alignItems: 'center',
            gap: 8,
            padding: '6px 12px',
            borderRadius: 8,
            border: '1px solid var(--color-border-secondary)',
            background: 'var(--color-bg-elevated)',
            cursor: 'pointer',
            transition: 'all 0.2s',
            minHeight: 36,
          }}
          onMouseEnter={(e) => {
            (e.currentTarget as HTMLDivElement).style.borderColor = 'var(--color-primary-hover)';
            (e.currentTarget as HTMLDivElement).style.background = 'var(--color-bg-hover)';
          }}
          onMouseLeave={(e) => {
            (e.currentTarget as HTMLDivElement).style.borderColor = 'var(--color-border-secondary)';
            (e.currentTarget as HTMLDivElement).style.background = 'var(--color-bg-elevated)';
          }}
        >
          {selectedExpert ? (
            <>
              {/* 已选中：显示图标+名称+职业 */}
              {selectedExpert.expert_type === 'team' ? (
                <TeamOutlined style={{ color: 'var(--color-warning)', fontSize: 14 }} />
              ) : (
                <UserOutlined style={{ color: 'var(--color-info)', fontSize: 14 }} />
              )}
              <span
                style={{
                  fontSize: 14,
                  color: 'var(--color-text)',
                  overflow: 'hidden',
                  textOverflow: 'ellipsis',
                  whiteSpace: 'nowrap',
                }}
              >
                {getExpertDisplayName(selectedExpert)}
              </span>
              {getExpertProfession(selectedExpert) && (
                <span
                  style={{
                    fontSize: 12,
                    color: 'var(--color-text-secondary)',
                    flexShrink: 0,
                  }}
                >
                  {getExpertProfession(selectedExpert)}
                </span>
              )}
            </>
          ) : (
            <>
              <SearchOutlined style={{ fontSize: 14, color: 'var(--color-text-tertiary)' }} />
              <span style={{ fontSize: 14, color: 'var(--color-text-tertiary)' }}>
                选择专家/团队
              </span>
            </>
          )}
        </div>

        {/* 清除按钮：已有选择时显示，圆形背景更醒目 */}
        {selectedExpert && (
          <Tooltip title="清除选择">
            <div
              onClick={(e) => {
                e.stopPropagation();
                handleClear();
              }}
              role="button"
              tabIndex={0}
              onKeyDown={(e) => {
                if (e.key === 'Enter' || e.key === ' ') {
                  e.preventDefault();
                  e.stopPropagation();
                  handleClear();
                }
              }}
              style={{
                width: 24,
                height: 24,
                borderRadius: '50%',
                background: 'var(--color-bg-hover)',
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'center',
                cursor: 'pointer',
                flexShrink: 0,
                transition: 'all 0.15s',
              }}
              onMouseEnter={(e) => {
                (e.currentTarget as HTMLDivElement).style.background = 'var(--color-error-bg-1)';
                (e.currentTarget as HTMLDivElement).querySelectorAll('svg').forEach((el) => {
                  (el as SVGElement).style.color = 'var(--color-error)';
                });
              }}
              onMouseLeave={(e) => {
                (e.currentTarget as HTMLDivElement).style.background = 'var(--color-bg-hover)';
                (e.currentTarget as HTMLDivElement).querySelectorAll('svg').forEach((el) => {
                  (el as SVGElement).style.color = '';
                });
              }}
            >
              <CloseOutlined style={{ fontSize: 12, color: 'var(--color-text-secondary)' }} />
            </div>
          </Tooltip>
        )}
      </div>

      {/* 弹窗：搜索 + 专家列表 */}
      <Modal
        title={
          <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', paddingRight: 28 }}>
            <span>选择专家/团队</span>
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
        }
        open={open}
        onCancel={() => {
          setOpen(false);
          setKeyword('');
        }}
        footer={null}
        width={560}
        styles={{ body: { padding: '12px 0 0' } }}
      >
        {/* 搜索框 */}
        <div style={{ padding: '0 16px', marginBottom: 12 }}>
          <Input
            prefix={<SearchOutlined style={{ color: 'var(--color-text-tertiary)' }} />}
            placeholder="搜索专家名称、职业或描述"
            value={keyword}
            onChange={(e) => setKeyword(e.target.value)}
            allowClear
          />
        </div>

        {/* 专家列表 */}
        {loading ? (
          <div style={{ display: 'flex', justifyContent: 'center', padding: '32px 0' }}>
            <Spin size="small" />
          </div>
        ) : filteredExperts.length === 0 ? (
          <Empty
            description={keyword ? '没有匹配的专家' : '暂无专家'}
            image={Empty.PRESENTED_IMAGE_SIMPLE}
            style={{ padding: '32px 0' }}
          />
        ) : (
          <div style={{ maxHeight: 400, overflowY: 'auto', padding: '0 16px' }}>
            {filteredExperts.map((expert) => {
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
                    borderRadius: 8,
                    border: `1.5px solid ${selected ? 'var(--color-primary)' : 'transparent'}`,
                    background: selected ? 'var(--color-primary-bg-1)' : 'transparent',
                    cursor: 'pointer',
                    transition: 'all 0.15s ease',
                    marginBottom: 4,
                  }}
                  onMouseEnter={(e) => {
                    if (!selected) {
                      (e.currentTarget as HTMLDivElement).style.background = 'var(--color-primary-bg-2)';
                    }
                  }}
                  onMouseLeave={(e) => {
                    if (!selected) {
                      (e.currentTarget as HTMLDivElement).style.background = 'transparent';
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

                  {/* 团队/专家标识 */}
                  <div
                    style={{
                      fontSize: 10,
                      padding: '2px 6px',
                      borderRadius: 4,
                      background: isTeam ? 'var(--color-warning-bg-1)' : 'var(--color-info-bg-1)',
                      color: isTeam ? 'var(--color-warning)' : 'var(--color-info)',
                      flexShrink: 0,
                    }}
                  >
                    {isTeam ? '团队' : '专家'}
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </Modal>
    </div>
  );
});
