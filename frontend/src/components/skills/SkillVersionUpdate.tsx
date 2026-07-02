import { useState, useEffect, useMemo } from 'react';
import { Spin, Input, Card, Button, Tag, Modal, message, Empty, Tooltip } from 'antd';
import {
  SearchOutlined, SyncOutlined, CheckCircleOutlined,
  WarningOutlined,
} from '@ant-design/icons';
import { EXECUTORS } from '@/types';
import type { SkillVersionUpdate as SkillVersionUpdateType, SkillVersionInfo } from '@/types';
import * as db from '@/utils/database';
import { EXECUTOR_COLORS } from './helpers';

export function SkillVersionUpdate() {
  const [loading, setLoading] = useState(true);
  const [data, setData] = useState<SkillVersionUpdateType[]>([]);
  const [searchText, setSearchText] = useState('');
  const [confirmModalOpen, setConfirmModalOpen] = useState(false);
  const [selectedSkill, setSelectedSkill] = useState<SkillVersionUpdateType | null>(null);
  const [updating, setUpdating] = useState(false);

  const loadData = () => {
    setLoading(true);
    db.getSkillVersionUpdates()
      .then(setData)
      .catch(err => message.error('加载失败: ' + err.message))
      .finally(() => setLoading(false));
  };

  useEffect(() => {
    loadData();
  }, []);

  const filtered = useMemo(() => {
    if (!searchText) return data;
    const lower = searchText.toLowerCase();
    return data.filter(s =>
      s.skill_name.toLowerCase().includes(lower) ||
      s.description?.toLowerCase().includes(lower)
    );
  }, [data, searchText]);

  const stats = useMemo(() => {
    const updatable = data.filter(s => s.has_update).length;
    const latest = data.filter(s => !s.has_update).length;
    return { updatable, latest };
  }, [data]);

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
              <div style={{ fontSize: 12, color: 'var(--color-text-secondary, #475569)' }}>已最新</div>
              <div style={{ fontSize: 20, fontWeight: 600, lineHeight: 1.2, color: 'var(--color-text, #0f172a)' }}>
                {stats.latest}
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

      {/* Skill 列表 */}
      {filtered.length === 0 ? (
        <Empty description="没有需要更新的 Skills" />
      ) : (
        <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
          {filtered.map(skill => (
            <SkillVersionCard
              key={skill.skill_name}
              skill={skill}
              onUpdate={() => handleUpdateClick(skill)}
            />
          ))}
        </div>
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
      <div style={{
        display: 'flex',
        flexDirection: 'column',
        alignItems: 'center',
        padding: '8px 4px',
        borderRadius: 8,
        background: isLatest ? `${color}15` : 'var(--color-fill, #f8fafc)',
        border: `1px solid ${isLatest ? color : 'var(--color-border, #e2e8f0)'}`,
        transition: 'all 0.2s',
      }}>
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
