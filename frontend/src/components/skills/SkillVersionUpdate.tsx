import { useState, useEffect, useMemo } from 'react';
import { Spin, Input, Card, Button, Tag, Modal, message, Empty, Tooltip, Collapse } from 'antd';
import {
  SearchOutlined, SyncOutlined, CheckCircleOutlined,
  WarningOutlined, RightOutlined,
} from '@ant-design/icons';
import { EXECUTORS } from '@/types';
import type { SkillVersionUpdate as SkillVersionUpdateType, SkillVersionInfo, SkillComparison } from '@/types';
import * as db from '@/utils/database';
import { EXECUTOR_COLORS } from './helpers';

// 深色主题适配样式
const styles = `
  .executor-version-block {
    background: var(--color-fill-quaternary, #f1f5f9);
    border: 1px solid var(--color-border, #e2e8f0);
  }
  .executor-version-block--latest {
    background: var(--color-primary-bg, rgba(8, 145, 178, 0.1));
    border-color: var(--color-primary, #0891b2);
  }
  .executor-version-block--same {
    background: var(--color-fill-quaternary, #f1f5f9);
    border-color: var(--color-border, #e2e8f0);
  }
`;

// 从 SkillComparison 构建 SkillVersionUpdate（用于版本相同的 skill）
function buildSameVersionUpdate(comparison: SkillComparison): SkillVersionUpdateType | null {
  const versions: SkillVersionInfo[] = [];
  let firstVersion: string | null = null;
  let firstExecutor = '';

  for (const [executor, presence] of Object.entries(comparison.executors)) {
    if (presence.present) {
      const executorLabel = EXECUTORS.find(e => e.value === executor)?.label || executor;
      if (!firstVersion && presence.version) {
        firstVersion = presence.version;
        firstExecutor = executor;
      }
      versions.push({
        executor,
        executor_label: executorLabel,
        version: presence.version,
        modified_at: presence.modified_at,
        is_latest: true,
      });
    }
  }

  if (versions.length < 2) return null;

  return {
    skill_name: comparison.skill_name,
    description: comparison.description,
    versions,
    latest_version: firstVersion,
    latest_executor: firstExecutor,
    has_update: false,
  };
}

export function SkillVersionUpdate() {
  const [loading, setLoading] = useState(true);
  const [updateData, setUpdateData] = useState<SkillVersionUpdateType[]>([]);
  const [comparisonData, setComparisonData] = useState<SkillComparison[]>([]);
  const [searchText, setSearchText] = useState('');
  const [confirmModalOpen, setConfirmModalOpen] = useState(false);
  const [selectedSkill, setSelectedSkill] = useState<SkillVersionUpdateType | null>(null);
  const [updating, setUpdating] = useState(false);

  // 注入深色主题适配样式
  useEffect(() => {
    const styleEl = document.createElement('style');
    styleEl.textContent = styles;
    document.head.appendChild(styleEl);
    return () => {
      document.head.removeChild(styleEl);
    };
  }, []);

  const loadData = async () => {
    setLoading(true);
    try {
      const [updates, comparisons] = await Promise.all([
        db.getSkillVersionUpdates(),
        db.getSkillsComparison(),
      ]);
      setUpdateData(updates);
      setComparisonData(comparisons);
    } catch (err: unknown) {
      const errorMessage = err instanceof Error ? err.message : '加载失败';
      message.error(errorMessage);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    loadData();
  }, []);

  // 从 comparisonData 提取所有 skill（用于折叠区域）
  const allSkills = useMemo(() => {
    // 获取有版本差异的 skill 名称集合
    const updateSkillNames = new Set(updateData.map(s => s.skill_name));

    return comparisonData
      .filter(c => !updateSkillNames.has(c.skill_name))
      .map(buildSameVersionUpdate)
      .filter((s): s is SkillVersionUpdateType => s !== null);
  }, [comparisonData, updateData]);

  // 过滤有差异的 skill
  const filteredUpdates = useMemo(() => {
    if (!searchText) return updateData;
    const lower = searchText.toLowerCase();
    return updateData.filter(s =>
      s.skill_name.toLowerCase().includes(lower) ||
      s.description?.toLowerCase().includes(lower)
    );
  }, [updateData, searchText]);

  // 过滤所有 skill（用于折叠区域）
  const filteredAllSkills = useMemo(() => {
    if (!searchText) return allSkills;
    const lower = searchText.toLowerCase();
    return allSkills.filter(s =>
      s.skill_name.toLowerCase().includes(lower) ||
      s.description?.toLowerCase().includes(lower)
    );
  }, [allSkills, searchText]);

  const stats = useMemo(() => {
    const updatable = updateData.filter(s => s.has_update).length;
    return {
      updatable,
      total: updateData.length + allSkills.length,
    };
  }, [updateData, allSkills]);

  const handleUpdateClick = (skill: SkillVersionUpdateType) => {
    setSelectedSkill(skill);
    setConfirmModalOpen(true);
  };

  const handleConfirmUpdate = async () => {
    if (!selectedSkill) return;

    setUpdating(true);
    try {
      // 找出需要更新的执行器（非最新版本的）
      const targetExecutors = selectedSkill.versions
        .filter(v => !v.is_latest)
        .map(v => v.executor);

      await db.syncSkill(
        selectedSkill.latest_executor,
        selectedSkill.skill_name,
        targetExecutors
      );

      message.success(`已将 ${selectedSkill.skill_name} 更新到 v${selectedSkill.latest_version}`);
      setConfirmModalOpen(false);
      setSelectedSkill(null);
      loadData();
    } catch (err: unknown) {
      const errorMessage = err instanceof Error ? err.message : '更新失败';
      message.error(errorMessage);
    } finally {
      setUpdating(false);
    }
  };

  if (loading) {
    return <div style={{ textAlign: 'center', padding: 48 }}><Spin size="large" /></div>;
  }

  return (
    <div>
      {/* 统计卡片 */}
      <div style={{
        display: 'grid',
        gridTemplateColumns: 'repeat(2, 1fr)',
        gap: 12,
        marginBottom: 20,
      }}>
        <Card size="small" style={{ borderRadius: 12 }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
            <div style={{
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'center',
              width: 36,
              height: 36,
              borderRadius: 10,
              background: 'rgba(245, 158, 11, 0.12)',
              color: '#f59e0b',
              fontSize: 16,
            }}>
              <WarningOutlined />
            </div>
            <div>
              <div style={{ fontSize: 12, color: 'var(--color-text-secondary, #475569)' }}>可更新</div>
              <div style={{ fontSize: 20, fontWeight: 600, lineHeight: 1.2, color: 'var(--color-text, #0f172a)' }}>
                {stats.updatable}
              </div>
            </div>
          </div>
        </Card>
        <Card size="small" style={{ borderRadius: 12 }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
            <div style={{
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'center',
              width: 36,
              height: 36,
              borderRadius: 10,
              background: 'rgba(16, 185, 129, 0.12)',
              color: '#10b981',
              fontSize: 16,
            }}>
              <CheckCircleOutlined />
            </div>
            <div>
              <div style={{ fontSize: 12, color: 'var(--color-text-secondary, #475569)' }}>版本相同</div>
              <div style={{ fontSize: 20, fontWeight: 600, lineHeight: 1.2, color: 'var(--color-text, #0f172a)' }}>
                {stats.total - stats.updatable}
              </div>
            </div>
          </div>
        </Card>
      </div>

      {/* 搜索框 */}
      <div style={{ marginBottom: 16 }}>
        <Input
          placeholder="搜索 Skills..."
          prefix={<SearchOutlined style={{ color: 'var(--color-text-quaternary, #94a3b8)' }} />}
          value={searchText}
          onChange={e => setSearchText(e.target.value)}
          style={{ width: 200, borderRadius: 20 }}
          allowClear
        />
      </div>

      {/* 有版本差异的 Skill 列表 */}
      {filteredUpdates.length === 0 && filteredAllSkills.length === 0 ? (
        <Empty description="没有匹配的 Skills" />
      ) : (
        <>
          {filteredUpdates.length > 0 && (
            <div style={{ display: 'flex', flexDirection: 'column', gap: 12, marginBottom: 16 }}>
              {filteredUpdates.map(skill => (
                <SkillVersionCard
                  key={skill.skill_name}
                  skill={skill}
                  onUpdate={() => handleUpdateClick(skill)}
                />
              ))}
            </div>
          )}

          {/* 所有 Skill 折叠区域 */}
          {filteredAllSkills.length > 0 && (
            <Collapse
              defaultActiveKey={[]}
              expandIcon={({ isActive }) => (
                <RightOutlined rotate={isActive ? 90 : 0} />
              )}
              style={{
                borderRadius: 12,
                border: '1px solid var(--color-border, #e2e8f0)',
              }}
              items={[{
                key: 'all-skills',
                label: (
                  <span style={{ fontSize: 14, color: 'var(--color-text, #0f172a)' }}>
                    其他 Skills ({filteredAllSkills.length})
                  </span>
                ),
                children: (
                  <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
                    {filteredAllSkills.map(skill => (
                      <SkillVersionCard
                        key={skill.skill_name}
                        skill={skill}
                        onUpdate={() => {}}
                      />
                    ))}
                  </div>
                ),
                showArrow: true,
              }]}
            />
          )}
        </>
      )}

      {/* 确认更新弹窗 */}
      <Modal
        title="确认更新 Skill"
        open={confirmModalOpen}
        onOk={handleConfirmUpdate}
        onCancel={() => {
          setConfirmModalOpen(false);
          setSelectedSkill(null);
        }}
        confirmLoading={updating}
        okText="确认更新"
        cancelText="取消"
      >
        {selectedSkill && (
          <div>
            <p style={{ marginBottom: 16 }}>
              将 <strong>{selectedSkill.skill_name}</strong> 从{' '}
              <Tag color="blue">{selectedSkill.latest_executor}</Tag>{' '}
              (v{selectedSkill.latest_version}) 同步到以下执行器：
            </p>
            <ul style={{ marginBottom: 16, paddingLeft: 20 }}>
              {selectedSkill.versions
                .filter(v => !v.is_latest)
                .map(v => (
                  <li key={v.executor} style={{ marginBottom: 8 }}>
                    <Tag>{v.executor_label}</Tag>
                    <span style={{ color: '#94a3b8' }}>
                      {v.version ? `v${v.version}` : '无版本'} → v{selectedSkill.latest_version}
                    </span>
                  </li>
                ))}
            </ul>
            <p style={{ color: '#f59e0b', fontSize: 12 }}>
              <WarningOutlined /> 此操作将覆盖目标执行器的同名 skill
            </p>
          </div>
        )}
      </Modal>
    </div>
  );
}

function SkillVersionCard({ skill, onUpdate }: {
  skill: SkillVersionUpdateType;
  onUpdate: () => void;
}) {
  const latestExecutorLabel = EXECUTORS.find(e => e.value === skill.latest_executor)?.label || skill.latest_executor;

  return (
    <Card
      size="small"
      style={{
        borderRadius: 12,
        borderColor: skill.has_update ? 'rgba(245, 158, 11, 0.3)' : 'var(--color-border, #e2e8f0)',
      }}
      styles={{ body: { padding: 16 } }}
    >
      {/* 头部：名称 + 版本 + 操作按钮 */}
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 12 }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
          <span style={{ fontSize: 14, fontWeight: 500, color: 'var(--color-text, #0f172a)' }}>
            {skill.skill_name}
          </span>
          {skill.latest_version && (
            <Tag color="blue" style={{ margin: 0 }}>
              最新: v{skill.latest_version}
            </Tag>
          )}
          <Tag color="green" style={{ margin: 0 }}>
            {latestExecutorLabel}
          </Tag>
        </div>
        {skill.has_update && (
          <Button
            type="primary"
            size="small"
            icon={<SyncOutlined />}
            onClick={onUpdate}
            style={{ borderRadius: 16 }}
          >
            全部更新到 v{skill.latest_version}
          </Button>
        )}
      </div>

      {/* 描述 */}
      {skill.description && (
        <div style={{
          fontSize: 12,
          color: 'var(--color-text-secondary, #475569)',
          marginBottom: 12,
          lineHeight: 1.5,
        }}>
          {skill.description}
        </div>
      )}

      {/* 执行器版本分布 */}
      <div style={{
        display: 'grid',
        gridTemplateColumns: 'repeat(auto-fill, minmax(120px, 1fr))',
        gap: 8,
      }}>
        {skill.versions.map(v => (
          <ExecutorVersionBlock key={v.executor} versionInfo={v} />
        ))}
      </div>
    </Card>
  );
}

function ExecutorVersionBlock({ versionInfo }: { versionInfo: SkillVersionInfo }) {
  const color = EXECUTOR_COLORS[versionInfo.executor] || '#64748b';
  const isLatest = versionInfo.is_latest;

  return (
    <Tooltip title={`${versionInfo.executor_label}${isLatest ? ' (最新)' : ''}`}>
      <div className={`executor-version-block ${isLatest ? 'executor-version-block--latest' : ''}`}
        style={{
          display: 'flex',
          flexDirection: 'column',
          alignItems: 'center',
          padding: '8px 4px',
          borderRadius: 8,
          transition: 'all 0.2s',
        }}
      >
        <div style={{
          fontSize: 11,
          color: 'var(--color-text-secondary, #475569)',
          marginBottom: 4,
          textAlign: 'center',
          overflow: 'hidden',
          textOverflow: 'ellipsis',
          whiteSpace: 'nowrap',
          maxWidth: '100%',
        }}>
          {versionInfo.executor_label}
        </div>
        <div style={{
          fontSize: 12,
          fontWeight: 500,
          color: isLatest ? color : 'var(--color-text, #0f172a)',
        }}>
          {versionInfo.version ? `v${versionInfo.version}` : '-'}
        </div>
        {isLatest && (
          <CheckCircleOutlined style={{ fontSize: 12, color, marginTop: 4 }} />
        )}
        {!isLatest && versionInfo.version && (
          <WarningOutlined style={{ fontSize: 12, color: '#f59e0b', marginTop: 4 }} />
        )}
      </div>
    </Tooltip>
  );
}
