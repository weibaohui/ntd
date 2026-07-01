import { useState, useEffect, useMemo } from 'react';
import { Spin, Input, Space, Button, Tag, Dropdown, Card, message } from 'antd';
import type { MenuProps } from 'antd';
import {
  ThunderboltOutlined, SearchOutlined,
  DownloadOutlined, ExportOutlined, ImportOutlined,
  AppstoreOutlined, FileTextOutlined,
  UnorderedListOutlined, AppstoreOutlined as AppstoreOutlinedIcon,
} from '@ant-design/icons';
import { EXECUTORS } from '@/types';
import type { SkillMeta, ExecutorSkills } from '@/types';
import * as db from '@/utils/database';
import { EXECUTOR_COLORS, formatSize, splitSkillName } from './helpers';
import { SkillDetailDrawer } from './SkillDetailDrawer';
import { ImportExportModal } from './ImportExportModal';
import { SkillCardView } from './SkillCardView';

export function SkillsOverview() {
  const [loading, setLoading] = useState(true);
  const [data, setData] = useState<ExecutorSkills[]>([]);
  const [searchText, setSearchText] = useState('');
  const [filterExecutor, setFilterExecutor] = useState<string>('all');
  const [selectedSkill, setSelectedSkill] = useState<SkillMeta | null>(null);
  const [selectedExecutor, setSelectedExecutor] = useState('');
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [exportModalOpen, setExportModalOpen] = useState(false);
  const [exportMode, setExportMode] = useState<'import' | 'export'>('export');
  const [initialSelectedSkills, setInitialSelectedSkills] = useState<string[] | undefined>(undefined);
  const [viewMode, setViewMode] = useState<'list' | 'card'>('card');

  const loadData = () => {
    setLoading(true);
    db.getSkillsList()
      .then(data => {
        setData(data);
        setSelectedExecutor(prev => {
          if (prev && data.some(e => e.executor === prev)) return prev;
          const withSkills = data.find(e => e.skills.length > 0);
          return withSkills?.executor ?? data[0]?.executor ?? '';
        });
      })
      .catch(err => message.error('加载失败: ' + err.message))
      .finally(() => setLoading(false));
  };

  useEffect(() => {
    loadData();
  }, []);

  const handleSkillClick = (skill: SkillMeta, executor: string) => {
    setSelectedSkill(skill);
    setSelectedExecutor(executor);
    setDrawerOpen(true);
  };

  // Stats computed from unfiltered data (total counts, not affected by filter)
  const stats = useMemo(() => {
    const totalSkills = data.reduce((sum, e) => sum + e.skills.length, 0);
    const totalFiles = data.reduce((sum, e) => sum + e.skills.reduce((s, sk) => s + sk.file_count, 0), 0);
    const executorsWithSkills = data.filter(e => e.skills.length > 0).length;
    return { totalSkills, totalFiles, executorsWithSkills };
  }, [data]);

  // All skills flattened (filtered by executor, but not by search)
  const allSkills = useMemo(() => {
    const skills: { skill: SkillMeta; executor: string }[] = [];
    data.forEach(e => {
      e.skills.forEach(s => {
        if (filterExecutor === 'all' || filterExecutor === e.executor) {
          skills.push({ skill: s, executor: e.executor });
        }
      });
    });
    return skills;
  }, [data, filterExecutor]);

  // Skills filtered by both executor and search text
  const filteredSkills = useMemo(() => {
    if (!searchText) return allSkills;
    const lower = searchText.toLowerCase();
    return allSkills.filter(
      ({ skill }) =>
        skill.name.toLowerCase().includes(lower) ||
        skill.description?.toLowerCase().includes(lower) ||
        skill.keywords?.some(k => k.toLowerCase().includes(lower))
    );
  }, [allSkills, searchText]);

  // Executor filter tabs — counts reflect each executor's own skill total,
  // only narrowed by search text; never zeroed out by executor filter selection.
  const executorTabs = useMemo(() => {
    const matchSearch = (skills: SkillMeta[]) => {
      if (!searchText) return skills.length;
      const lower = searchText.toLowerCase();
      return skills.filter(s =>
        s.name.toLowerCase().includes(lower) ||
        s.description?.toLowerCase().includes(lower) ||
        s.keywords?.some(k => k.toLowerCase().includes(lower))
      ).length;
    };

    const tabs = [{ key: 'all', label: '全部', count: matchSearch(allSkills.map(s => s.skill)) }];
    data.forEach(e => {
      const label = EXECUTORS.find(x => x.value === e.executor)?.label || e.executor;
      tabs.push({ key: e.executor, label, count: matchSearch(e.skills) });
    });
    return tabs;
  }, [data, searchText, allSkills]);

  const exportMenuItems: MenuProps['items'] = [
    { key: 'export', icon: <ExportOutlined />, label: '导出选中' },
    { key: 'export-all', icon: <ExportOutlined />, label: '导出全部' },
    { type: 'divider' },
    { key: 'import', icon: <ImportOutlined />, label: '导入' },
  ];

  const handleExportMenuClick: MenuProps['onClick'] = ({ key }) => {
    if (key === 'import') {
      setExportMode('import');
      setInitialSelectedSkills(undefined);
    } else {
      setExportMode('export');
      if (key === 'export-all') {
        // Export from current filter scope, not stale selectedExecutor
        const scopeExecutor = filterExecutor !== 'all' ? filterExecutor : selectedExecutor;
        const executorData = data.find(e => e.executor === scopeExecutor);
        if (executorData) {
          setInitialSelectedSkills(executorData.skills.map(s => s.name));
        }
      } else {
        setInitialSelectedSkills(undefined);
      }
    }
    setExportModalOpen(true);
  };

  if (loading) {
    return <div style={{ textAlign: 'center', padding: 48 }}><Spin size="large" /></div>;
  }

  return (
    <div>
      {/* Stats row */}
      <div style={{
        display: 'grid',
        gridTemplateColumns: 'repeat(3, 1fr)',
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
              background: 'rgba(8, 145, 178, 0.12)',
              color: '#0891b2',
              fontSize: 16,
            }}>
              <ThunderboltOutlined />
            </div>
            <div>
              <div style={{ fontSize: 12, color: 'var(--color-text-secondary, #475569)' }}>Skill 总数</div>
              <div style={{ fontSize: 20, fontWeight: 600, lineHeight: 1.2, color: 'var(--color-text, #0f172a)' }}>
                {stats.totalSkills}
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
              <AppstoreOutlined />
            </div>
            <div>
              <div style={{ fontSize: 12, color: 'var(--color-text-secondary, #475569)' }}>执行器</div>
              <div style={{ fontSize: 20, fontWeight: 600, lineHeight: 1.2, color: 'var(--color-text, #0f172a)' }}>
                {stats.executorsWithSkills}
                <span style={{ fontSize: 13, fontWeight: 400, marginLeft: 2 }}>/ {data.length}</span>
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
              background: 'rgba(245, 158, 11, 0.12)',
              color: '#f59e0b',
              fontSize: 16,
            }}>
              <FileTextOutlined />
            </div>
            <div>
              <div style={{ fontSize: 12, color: 'var(--color-text-secondary, #475569)' }}>文件总数</div>
              <div style={{ fontSize: 20, fontWeight: 600, lineHeight: 1.2, color: 'var(--color-text, #0f172a)' }}>
                {stats.totalFiles}
              </div>
            </div>
          </div>
        </Card>
      </div>

      {/* Search & Action bar - 始终显示 */}
      <div style={{
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'space-between',
        marginBottom: 16,
        gap: 12,
        flexWrap: 'wrap',
      }}>
        <Input
          placeholder="搜索 Skills..."
          prefix={<SearchOutlined style={{ color: 'var(--color-text-quaternary, #94a3b8)' }} />}
          value={searchText}
          onChange={e => setSearchText(e.target.value)}
          style={{ width: 200, borderRadius: 20 }}
          allowClear
        />

        <Space size={8}>
          {/* 视图切换按钮 */}
          <div style={{
            display: 'flex',
            alignItems: 'center',
            border: '1px solid var(--color-border, #e2e8f0)',
            borderRadius: 8,
            overflow: 'hidden',
          }}>
            <button
              onClick={() => setViewMode('list')}
              title="列表视图"
              style={{
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'center',
                width: 32,
                height: 32,
                border: 'none',
                background: viewMode === 'list' ? 'var(--color-fill, #e2e8f0)' : 'transparent',
                color: viewMode === 'list' ? 'var(--color-text, #0f172a)' : 'var(--color-text-tertiary, #94a3b8)',
                cursor: 'pointer',
                transition: 'all 0.2s',
              }}
            >
              <UnorderedListOutlined />
            </button>
            <button
              onClick={() => setViewMode('card')}
              title="卡片视图"
              style={{
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'center',
                width: 32,
                height: 32,
                border: 'none',
                borderLeft: '1px solid var(--color-border, #e2e8f0)',
                background: viewMode === 'card' ? 'var(--color-fill, #e2e8f0)' : 'transparent',
                color: viewMode === 'card' ? 'var(--color-text, #0f172a)' : 'var(--color-text-tertiary, #94a3b8)',
                cursor: 'pointer',
                transition: 'all 0.2s',
              }}
            >
              <AppstoreOutlinedIcon />
            </button>
          </div>
          <Dropdown menu={{ items: exportMenuItems, onClick: handleExportMenuClick }} trigger={['click']}>
            <Button
              type="primary"
              icon={<DownloadOutlined />}
              style={{ borderRadius: 20 }}
            >
              导入/导出
            </Button>
          </Dropdown>
        </Space>
      </div>

      {/* Executor filter pills - 仅列表模式显示 */}
      {viewMode === 'list' && (
        <div
          role="tablist"
          aria-label="按执行器筛选"
          style={{
            display: 'flex',
            gap: 6,
            flexWrap: 'wrap',
            marginBottom: 16,
          }}
        >
          {executorTabs.map(tab => {
            const isActive = filterExecutor === tab.key;
            const color = tab.key === 'all' ? '#0891b2' : (EXECUTOR_COLORS[tab.key] || '#64748b');
            return (
              <button
                key={tab.key}
                role="tab"
                aria-selected={isActive}
                onClick={() => setFilterExecutor(tab.key)}
                style={{
                  display: 'inline-flex',
                  alignItems: 'center',
                  gap: 4,
                  padding: '4px 12px',
                  borderRadius: 20,
                  border: `1px solid ${isActive ? color : 'var(--color-border, #e2e8f0)'}`,
                  background: isActive ? `${color}15` : 'transparent',
                  color: isActive ? color : 'var(--color-text-secondary, #475569)',
                  cursor: 'pointer',
                  fontSize: 13,
                  fontWeight: isActive ? 500 : 400,
                  transition: 'all 0.2s',
                  whiteSpace: 'nowrap',
                }}
              >
                {tab.label}
                <span style={{
                  display: 'inline-flex',
                  alignItems: 'center',
                  justifyContent: 'center',
                  minWidth: 18,
                  height: 18,
                  borderRadius: 9,
                  background: isActive ? color : 'var(--color-fill, #e2e8f0)',
                  color: isActive ? '#fff' : 'var(--color-text-secondary, #475569)',
                  fontSize: 11,
                  lineHeight: 1,
                  padding: '0 4px',
                }}>
                  {tab.count}
                </span>
              </button>
            );
          })}
        </div>
      )}

      {/* Skill content - 根据视图模式切换 */}
      {viewMode === 'card' ? (
        // 卡片视图：使用新的 SkillCardView 组件（搜索由父组件提供）
        <SkillCardView
          data={data}
          searchText={searchText}
          onSkillClick={handleSkillClick}
        />
      ) : (
        // 列表视图：保持原有的网格布局
        filteredSkills.length === 0 ? (
          <div style={{
            textAlign: 'center',
            padding: '60px 20px',
            color: 'var(--color-text-secondary, #475569)',
          }}>
            <AppstoreOutlined style={{ fontSize: 48, marginBottom: 16, opacity: 0.3 }} />
            <div style={{ fontSize: 16 }}>{searchText ? '无匹配结果' : '暂无 Skills'}</div>
          </div>
        ) : (
          <div style={{
            display: 'grid',
            gridTemplateColumns: 'repeat(auto-fill, minmax(280px, 1fr))',
            gap: 12,
          }}>
            {filteredSkills.map(({ skill, executor }) => (
              <SkillCard
                key={`${executor}-${skill.name}`}
                skill={skill}
                executor={executor}
                onClick={() => handleSkillClick(skill, executor)}
              />
            ))}
          </div>
        )
      )}

      <SkillDetailDrawer
        skill={selectedSkill}
        executor={selectedExecutor}
        executorLabel={EXECUTORS.find(e => e.value === selectedExecutor)?.label || selectedExecutor}
        open={drawerOpen}
        onClose={() => setDrawerOpen(false)}
        onSyncSuccess={loadData}
        onDeleteSuccess={loadData}
      />

      <ImportExportModal
        open={exportModalOpen}
        mode={exportMode}
        executor={filterExecutor !== 'all' ? filterExecutor : selectedExecutor}
        data={data}
        initialSelectedSkills={initialSelectedSkills}
        onClose={() => {
          setExportModalOpen(false);
          setInitialSelectedSkills(undefined);
        }}
      />
    </div>
  );
}

function SkillCard({ skill, executor, onClick }: {
  skill: SkillMeta;
  executor: string;
  onClick: () => void;
}) {
  const color = EXECUTOR_COLORS[executor] || '#0891b2';
  const { category, shortName } = splitSkillName(skill.name);
  const executorLabel = EXECUTORS.find(e => e.value === executor)?.label || executor;

  return (
    <Card
      size="small"
      hoverable
      onClick={onClick}
      tabIndex={0}
      role="button"
      aria-label={`${shortName} - ${executorLabel}`}
      onKeyDown={(e) => {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault();
          onClick();
        }
      }}
      className="skill-card"
      style={{
        borderRadius: 12,
        cursor: 'pointer',
        transition: 'all 0.2s',
        position: 'relative',
        overflow: 'hidden',
        borderColor: 'var(--color-border, #e2e8f0)',
      }}
      styles={{
        body: { padding: 16 },
      }}
    >
      {/* Top accent line */}
      <div style={{
        position: 'absolute',
        top: 0,
        left: 0,
        right: 0,
        height: 2,
        background: `linear-gradient(90deg, ${color}, ${color}60)`,
        opacity: 0.6,
      }} />

      {/* Header: icon + name + executor tag */}
      <div style={{ display: 'flex', alignItems: 'flex-start', gap: 10 }}>
        <div style={{
          width: 36,
          height: 36,
          borderRadius: 10,
          background: `${color}15`,
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center',
          color,
          fontSize: 15,
          fontWeight: 600,
          flexShrink: 0,
        }}>
          {shortName.charAt(0).toUpperCase()}
        </div>
        <div style={{ flex: 1, minWidth: 0 }}>
          <div style={{
            fontSize: 14,
            fontWeight: 500,
            color: 'var(--color-text, #0f172a)',
            overflow: 'hidden',
            textOverflow: 'ellipsis',
            whiteSpace: 'nowrap',
          }}>
            {shortName}
          </div>
          <div style={{
            fontSize: 11,
            color: 'var(--color-text-tertiary, #94a3b8)',
            marginTop: 2,
            overflow: 'hidden',
            textOverflow: 'ellipsis',
            whiteSpace: 'nowrap',
          }}>
            {executorLabel}
          </div>
        </div>
      </div>

      {/* Description */}
      {skill.description && (
        <div style={{
          fontSize: 12,
          color: 'var(--color-text-secondary, #475569)',
          lineHeight: 1.5,
          display: '-webkit-box',
          WebkitLineClamp: 2,
          WebkitBoxOrient: 'vertical',
          overflow: 'hidden',
          minHeight: 36,
          marginTop: 12,
        }}>
          {skill.description}
        </div>
      )}

      {/* Footer: tags */}
      <div style={{
        display: 'flex',
        alignItems: 'center',
        gap: 6,
        marginTop: 12,
        flexWrap: 'wrap',
      }}>
        {category && (
          <Tag style={{
            margin: 0,
            fontSize: 11,
            lineHeight: '18px',
            padding: '0 6px',
            borderRadius: 4,
            background: 'var(--color-fill, #e2e8f0)',
            border: 'none',
            color: 'var(--color-text-secondary, #475569)',
          }}>
            {category}
          </Tag>
        )}
        {skill.version && (
          <Tag style={{
            margin: 0,
            fontSize: 11,
            lineHeight: '18px',
            padding: '0 6px',
            borderRadius: 4,
            background: `${color}15`,
            border: 'none',
            color,
          }}>
            v{skill.version}
          </Tag>
        )}
        <span style={{
          marginLeft: 'auto',
          fontSize: 11,
          color: 'var(--color-text-quaternary, #94a3b8)',
        }}>
          {formatSize(skill.total_size)}
        </span>
      </div>
    </Card>
  );
}
