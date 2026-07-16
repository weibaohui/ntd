/**
 * 技能市场组件
 *
 * 支持两种视图模式：
 * 1. 「按来源浏览」模式：先展示来源卡片网格，点击来源进入该来源的技能列表
 * 2. 「全部技能」模式：展示所有技能卡片，顶部支持搜索 + 下拉来源卡片筛选
 *
 * 交互逻辑与「总览」一致：
 * - 点击技能卡片 → Drawer 详情 → 安装 → 选择执行器
 */
import { useState, useEffect, useMemo, useCallback } from 'react';
import {
  Card, Tag, Input, Empty, Spin, App,
  Drawer, Descriptions, Button, Space, Modal, Checkbox, Row, Col,
  Alert, Typography, Dropdown, Divider,
} from 'antd';
import {
  SearchOutlined, FileTextOutlined, DownloadOutlined,
  InfoCircleOutlined, FolderOutlined, AppstoreOutlined,
  UnorderedListOutlined, ArrowLeftOutlined, StarOutlined,
  LinkOutlined, DownOutlined,
} from '@ant-design/icons';
import XMarkdown from '@ant-design/x-markdown';
import { bundledApi, type BundledSkillMeta, type BundledSkillFile, type SkillSourceMeta } from '@/api/bundled';
import type { ExecutorSkills } from '@/types';
import { EXECUTORS } from '@/types';
import { formatSize, formatTime } from './helpers';
import { useTheme } from '@/hooks/useTheme';
import * as db from '@/utils/database';

const { Text, Paragraph } = Typography;

// ─────────────────────────────────────────────────────────────────────────────
// 视图模式
// ─────────────────────────────────────────────────────────────────────────────
type ViewMode = 'browse-sources' | 'all-skills';

// ─────────────────────────────────────────────────────────────────────────────
// 来源卡片（用于来源浏览视图和下拉筛选）
// ─────────────────────────────────────────────────────────────────────────────
function SourceCard({
  sourceKey,
  meta,
  skillCount,
  onClick,
  compact = false,
}: {
  sourceKey: string;
  meta?: SkillSourceMeta;
  skillCount: number;
  onClick: () => void;
  compact?: boolean;
}) {
  const name = meta?.display_name || sourceKey;

  return (
    <Card
      size="small"
      hoverable
      onClick={onClick}
      style={{
        borderRadius: 12,
        cursor: 'pointer',
        transition: 'all 0.2s',
        borderColor: 'var(--color-border, #e2e8f0)',
      }}
      styles={{ body: { padding: compact ? 12 : 16 } }}
    >
      {/* 名称 + Stars */}
      <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 8 }}>
        <span style={{
          fontSize: compact ? 13 : 15,
          fontWeight: 600,
          color: 'var(--color-text, #0f172a)',
          flex: 1,
          overflow: 'hidden',
          textOverflow: 'ellipsis',
          whiteSpace: 'nowrap',
        }}>
          {name}
        </span>
        {(meta?.stars ?? 0) > 0 && (
          <Tag color="warning" style={{ margin: 0, fontSize: 11 }}>
            <StarOutlined style={{ fontSize: 10 }} /> {formatStars(meta!.stars)}
          </Tag>
        )}
      </div>

      {/* 描述 */}
      {meta?.description && (
        <Paragraph
          ellipsis={{ rows: compact ? 2 : 3 }}
          style={{
            fontSize: 12,
            color: 'var(--color-text-secondary, #475569)',
            marginBottom: 8,
            lineHeight: 1.5,
          }}
        >
          {meta.description}
        </Paragraph>
      )}

      {/* GitHub + License + 技能数 */}
      <div style={{
        display: 'flex',
        alignItems: 'center',
        gap: 8,
        flexWrap: 'wrap',
      }}>
        {meta?.github_url && (
          <a
            href={meta.github_url}
            target="_blank"
            rel="noopener noreferrer"
            onClick={(e) => e.stopPropagation()}
            style={{
              fontSize: 11,
              color: '#7C3AED',
              display: 'inline-flex',
              alignItems: 'center',
              gap: 4,
            }}
          >
            <LinkOutlined style={{ fontSize: 10 }} /> GitHub
          </a>
        )}
        {meta?.license && (
          <Tag style={{ margin: 0, fontSize: 10, lineHeight: '16px' }}>
            {meta.license}
          </Tag>
        )}
        <span style={{
          marginLeft: 'auto',
          fontSize: 11,
          color: 'var(--color-text-quaternary, #94a3b8)',
        }}>
          {skillCount} 个技能
        </span>
      </div>
    </Card>
  );
}

// ─────────────────────────────────────────────────────────────────────────────
// 技能卡片
// ─────────────────────────────────────────────────────────────────────────────
function MarketSkillCard({ skill, installedExecutors, onClick }: {
  skill: BundledSkillMeta;
  installedExecutors: string[];
  onClick: () => void;
}) {
  const color = '#7C3AED';
  const isInstalled = installedExecutors.length > 0;

  return (
    <Card
      size="small"
      hoverable
      onClick={onClick}
      tabIndex={0}
      role="button"
      aria-label={skill.short_name}
      onKeyDown={(e) => {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault();
          onClick();
        }
      }}
      style={{
        borderRadius: 12,
        cursor: 'pointer',
        transition: 'all 0.2s',
        position: 'relative',
        overflow: 'hidden',
        borderColor: 'var(--color-border, #e2e8f0)',
      }}
      styles={{ body: { padding: 16 } }}
    >
      {/* 顶部装饰线 */}
      <div style={{
        position: 'absolute',
        top: 0, left: 0, right: 0,
        height: 2,
        background: `linear-gradient(90deg, ${color}, ${color}60)`,
        opacity: 0.6,
      }} />

      {/* 头部：图标 + 名称 + 来源标签 */}
      <div style={{ display: 'flex', alignItems: 'flex-start', gap: 10 }}>
        <div style={{
          width: 36, height: 36, borderRadius: 10,
          background: `${color}15`,
          display: 'flex', alignItems: 'center', justifyContent: 'center',
          color, fontSize: 15, fontWeight: 600, flexShrink: 0,
        }}>
          {skill.short_name.charAt(0).toUpperCase()}
        </div>
        <div style={{ flex: 1, minWidth: 0 }}>
          <div style={{
            fontSize: 14, fontWeight: 500,
            color: 'var(--color-text, #0f172a)',
            overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap',
          }}>
            {skill.short_name}
          </div>
          <div style={{
            fontSize: 11,
            color: 'var(--color-text-tertiary, #94a3b8)',
            marginTop: 2,
            overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap',
          }}>
            {skill.source_meta?.display_name || skill.source}
          </div>
        </div>
        {isInstalled && (
          <Tag color="green" style={{ fontSize: 10, margin: 0 }}>已安装</Tag>
        )}
      </div>

      {/* 描述 */}
      {(skill.description_zh || skill.description) && (
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
          {skill.description_zh || skill.description}
        </div>
      )}

      {/* 底部标签 */}
      <div style={{
        display: 'flex', alignItems: 'center', gap: 6,
        marginTop: 12, flexWrap: 'wrap',
      }}>
        {skill.version && (
          <Tag style={{
            margin: 0, fontSize: 11, lineHeight: '18px', padding: '0 6px',
            borderRadius: 4, background: `${color}15`, border: 'none', color,
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

// ─────────────────────────────────────────────────────────────────────────────
// 主组件
// ─────────────────────────────────────────────────────────────────────────────
export function SkillMarketplace() {
  const { message } = App.useApp();
  const { themeMode } = useTheme();
  const isDark = themeMode === 'dark';

  // ── 数据状态 ──
  const [skills, setSkills] = useState<BundledSkillMeta[]>([]);
  const [sources, setSources] = useState<Record<string, SkillSourceMeta>>({});
  const [loading, setLoading] = useState(false);

  // ── 视图状态 ──
  const [viewMode, setViewMode] = useState<ViewMode>('browse-sources');
  const [activeSource, setActiveSource] = useState<string | null>(null);

  // ── 筛选状态 ──
  const [searchText, setSearchText] = useState('');
  const [filterSource, setFilterSource] = useState<string>('all');

  // ── 详情 Drawer 状态 ──
  const [selectedSkill, setSelectedSkill] = useState<BundledSkillMeta | null>(null);
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [content, setContent] = useState('');
  const [files, setFiles] = useState<BundledSkillFile[]>([]);
  const [contentLoading, setContentLoading] = useState(false);

  // ── 安装 Modal 状态 ──
  const [installModalOpen, setInstallModalOpen] = useState(false);
  const [targetExecutors, setTargetExecutors] = useState<string[]>([]);
  const [installing, setInstalling] = useState(false);

  // ── 已安装技能数据 ──
  const [installedData, setInstalledData] = useState<ExecutorSkills[]>([]);

  /**
   * 加载市场技能列表
   */
  const loadSkills = useCallback(async () => {
    setLoading(true);
    try {
      const res = await bundledApi.getSkills();
      setSkills(res.skills);
      setSources(res.sources);
    } catch (e: any) {
      message.error('加载技能列表失败: ' + (e?.message || e));
    } finally {
      setLoading(false);
    }
  }, [message]);

  /**
   * 加载已安装技能
   */
  const loadInstalled = useCallback(async () => {
    try {
      const data = await db.getSkillsList();
      setInstalledData(data);
    } catch {
      // 静默失败
    }
  }, []);

  useEffect(() => {
    loadSkills();
    loadInstalled();
  }, [loadSkills, loadInstalled]);

  // ── 派生数据 ──
  const sourceNames = useMemo(() => {
    const set = new Set<string>();
    skills.forEach(s => set.add(s.source));
    return Array.from(set).sort();
  }, [skills]);

  const skillsBySource = useMemo(() => {
    const map: Record<string, BundledSkillMeta[]> = {};
    skills.forEach(s => {
      if (!map[s.source]) map[s.source] = [];
      map[s.source].push(s);
    });
    return map;
  }, [skills]);

  const filteredSkills = useMemo(() => {
    let result = skills;
    if (filterSource !== 'all') {
      result = result.filter(s => s.source === filterSource);
    }
    if (searchText) {
      const lower = searchText.toLowerCase();
      result = result.filter(s =>
        s.name.toLowerCase().includes(lower) ||
        s.short_name.toLowerCase().includes(lower) ||
        s.description?.toLowerCase().includes(lower) ||
        s.description_zh?.toLowerCase().includes(lower)
      );
    }
    return result;
  }, [skills, filterSource, searchText]);

  const activeSourceSkills = useMemo(() => {
    if (!activeSource) return [];
    let result = skillsBySource[activeSource] || [];
    if (searchText) {
      const lower = searchText.toLowerCase();
      result = result.filter(s =>
        s.name.toLowerCase().includes(lower) ||
        s.short_name.toLowerCase().includes(lower) ||
        s.description?.toLowerCase().includes(lower) ||
        s.description_zh?.toLowerCase().includes(lower)
      );
    }
    return result;
  }, [skillsBySource, activeSource, searchText]);

  // ── 判断已安装 ──
  const getInstalledExecutors = (skill: BundledSkillMeta): string[] => {
    const shortName = skill.short_name;
    const result: string[] = [];
    installedData.forEach(e => {
      if (e.skills.some(s => s.name === shortName || s.name === skill.name)) {
        result.push(e.executor);
      }
    });
    return result;
  };

  // ── 点击技能卡片 ──
  const handleCardClick = async (skill: BundledSkillMeta) => {
    setSelectedSkill(skill);
    setDrawerOpen(true);
    setContent('');
    setFiles([]);
    setContentLoading(true);
    try {
      const res = await bundledApi.getSkillContent(skill.name);
      setContent(res.content);
      setFiles(res.files);
    } catch {
      setContent('加载内容失败');
    } finally {
      setContentLoading(false);
    }
  };

  // ── 安装 ──
  const handleOpenInstall = () => {
    setTargetExecutors([]);
    setInstallModalOpen(true);
  };

  const handleInstall = async () => {
    if (!selectedSkill || targetExecutors.length === 0) return;
    setInstalling(true);
    const shortName = selectedSkill.short_name;
    const results: string[] = [];
    for (const executor of targetExecutors) {
      try {
        await bundledApi.installSkill(selectedSkill.name, executor);
        results.push(`${executor}: 成功`);
      } catch (e: any) {
        results.push(`${executor}: 失败 (${e?.message || e})`);
      }
    }
    setInstalling(false);
    setInstallModalOpen(false);
    const successCount = results.filter(r => r.includes('成功')).length;
    if (successCount === targetExecutors.length) {
      message.success(`${shortName} 已安装到 ${successCount} 个执行器`);
    } else {
      message.warning(`安装完成: ${successCount}/${targetExecutors.length} 成功`);
    }
    loadInstalled();
  };

  // ── 切换视图 ──
  const switchToSourceBrowse = () => {
    setViewMode('browse-sources');
    setActiveSource(null);
    setSearchText('');
  };

  const switchToAllSkills = () => {
    setViewMode('all-skills');
    setActiveSource(null);
    setFilterSource('all');
    setSearchText('');
  };

  const enterSource = (sourceKey: string) => {
    setActiveSource(sourceKey);
    setSearchText('');
  };

  // ── 下拉筛选内容 ──
  const dropdownContent = (
    <div style={{
      width: 520,
      maxHeight: 400,
      overflowY: 'auto',
      padding: 12,
      background: isDark ? '#1f1f2e' : '#fff',
      borderRadius: 12,
      boxShadow: '0 8px 32px rgba(0,0,0,0.15)',
    }}>
      <div style={{
        display: 'grid',
        gridTemplateColumns: 'repeat(2, 1fr)',
        gap: 10,
      }}>
        {/* 全部来源选项 */}
        <Card
          size="small"
          hoverable
          onClick={() => { setFilterSource('all'); }}
          style={{
            borderRadius: 10,
            cursor: 'pointer',
            borderColor: filterSource === 'all' ? '#0891b2' : 'var(--color-border, #e2e8f0)',
            background: filterSource === 'all' ? 'rgba(8,145,178,0.08)' : undefined,
          }}
          styles={{ body: { padding: 12 } }}
        >
          <div style={{ fontSize: 14, fontWeight: 500 }}>全部来源</div>
          <div style={{ fontSize: 11, color: 'var(--color-text-tertiary)', marginTop: 4 }}>
            {skills.length} 个技能
          </div>
        </Card>

        {sourceNames.map(src => (
          <SourceCard
            key={src}
            sourceKey={src}
            meta={sources[src]}
            skillCount={skillsBySource[src]?.length || 0}
            onClick={() => { setFilterSource(src); }}
            compact
          />
        ))}
      </div>
    </div>
  );

  // ── 顶部工具栏 ──
  const toolbar = (
    <div style={{
      display: 'flex',
      alignItems: 'center',
      gap: 12,
      marginBottom: 16,
      flexWrap: 'wrap',
    }}>
      {/* 视图切换 */}
      <Space>
        <Button
          type={viewMode === 'browse-sources' ? 'primary' : 'default'}
          icon={<AppstoreOutlined />}
          onClick={switchToSourceBrowse}
        >
          按来源浏览
        </Button>
        <Button
          type={viewMode === 'all-skills' ? 'primary' : 'default'}
          icon={<UnorderedListOutlined />}
          onClick={switchToAllSkills}
        >
          全部技能
        </Button>
      </Space>

      <Divider type="vertical" style={{ height: 24 }} />

      {/* 搜索框 */}
      <Input
        placeholder="搜索技能..."
        prefix={<SearchOutlined style={{ color: 'var(--color-text-quaternary, #94a3b8)' }} />}
        value={searchText}
        onChange={e => setSearchText(e.target.value)}
        style={{ width: 220, borderRadius: 20 }}
        allowClear
      />

      {/* 全部技能模式下的来源下拉筛选 */}
      {viewMode === 'all-skills' && (
        <Dropdown
          dropdownRender={() => dropdownContent}
          placement="bottomLeft"
          trigger={['click']}
        >
          <Button>
            {filterSource === 'all' ? '全部来源' : (sources[filterSource]?.display_name || filterSource)}
            <DownOutlined style={{ fontSize: 12, marginLeft: 4 }} />
          </Button>
        </Dropdown>
      )}

      {/* 来源浏览模式下，进入某个来源后的返回按钮 */}
      {viewMode === 'browse-sources' && activeSource && (
        <Button
          icon={<ArrowLeftOutlined />}
          onClick={() => setActiveSource(null)}
        >
          返回来源列表
        </Button>
      )}

      <Text type="secondary" style={{ marginLeft: 'auto', fontSize: 13 }}>
        {viewMode === 'browse-sources' && !activeSource
          ? `${sourceNames.length} 个来源`
          : `${viewMode === 'browse-sources' ? activeSourceSkills.length : filteredSkills.length} 个技能`
        }
      </Text>
    </div>
  );

  // ── 内容区域 ──
  const contentArea = (() => {
    // 按来源浏览 → 来源列表
    if (viewMode === 'browse-sources' && !activeSource) {
      if (loading) {
        return (
          <div style={{ textAlign: 'center', padding: 60 }}>
            <Spin size="large" />
          </div>
        );
      }
      if (sourceNames.length === 0) {
        return (
          <div style={{ textAlign: 'center', padding: '60px 20px' }}>
            <Empty description="暂无技能来源" />
          </div>
        );
      }
      return (
        <div style={{
          display: 'grid',
          gridTemplateColumns: 'repeat(auto-fill, minmax(300px, 1fr))',
          gap: 16,
        }}>
          {sourceNames.map(src => (
            <SourceCard
              key={src}
              sourceKey={src}
              meta={sources[src]}
              skillCount={skillsBySource[src]?.length || 0}
              onClick={() => enterSource(src)}
            />
          ))}
        </div>
      );
    }

    // 按来源浏览 → 某个来源的技能列表
    if (viewMode === 'browse-sources' && activeSource) {
      const sourceSkills = activeSourceSkills;
      const sourceMeta = sources[activeSource];
      return (
        <div>
          {/* 来源信息头 */}
          <div style={{
            marginBottom: 16,
            padding: '12px 16px',
            background: isDark ? 'rgba(255,255,255,0.04)' : 'rgba(0,0,0,0.02)',
            borderRadius: 10,
            border: `1px solid ${isDark ? 'rgba(255,255,255,0.08)' : 'rgba(0,0,0,0.06)'}`,
          }}>
            <Space wrap>
              <Text strong style={{ fontSize: 15 }}>
                {sourceMeta?.display_name || activeSource}
              </Text>
              {(sourceMeta?.stars ?? 0) > 0 && (
                <Tag color="warning">
                  <StarOutlined /> {formatStars(sourceMeta!.stars)}
                </Tag>
              )}
              {sourceMeta?.license && <Tag>{sourceMeta.license}</Tag>}
              {sourceMeta?.github_url && (
                <a href={sourceMeta.github_url} target="_blank" rel="noopener noreferrer">
                  <LinkOutlined /> GitHub
                </a>
              )}
            </Space>
            {sourceMeta?.description && (
              <div style={{
                fontSize: 12,
                color: 'var(--color-text-secondary, #475569)',
                marginTop: 6,
                lineHeight: 1.5,
              }}>
                {sourceMeta.description}
              </div>
            )}
          </div>

          {/* 技能卡片 */}
          {sourceSkills.length === 0 ? (
            <div style={{ textAlign: 'center', padding: 40 }}>
              <Empty description={searchText ? '无匹配结果' : '暂无技能'} />
            </div>
          ) : (
            <div style={{
              display: 'grid',
              gridTemplateColumns: 'repeat(auto-fill, minmax(280px, 1fr))',
              gap: 12,
            }}>
              {sourceSkills.map(skill => (
                <MarketSkillCard
                  key={skill.name}
                  skill={skill}
                  installedExecutors={getInstalledExecutors(skill)}
                  onClick={() => handleCardClick(skill)}
                />
              ))}
            </div>
          )}
        </div>
      );
    }

    // 全部技能模式
    return (
      <Spin spinning={loading}>
        {filteredSkills.length === 0 ? (
          <div style={{
            textAlign: 'center',
            padding: '60px 20px',
            color: 'var(--color-text-secondary, #475569)',
          }}>
            <Empty description={searchText ? '无匹配结果' : '暂无技能'} />
          </div>
        ) : (
          <div style={{
            display: 'grid',
            gridTemplateColumns: 'repeat(auto-fill, minmax(280px, 1fr))',
            gap: 12,
          }}>
            {filteredSkills.map(skill => (
              <MarketSkillCard
                key={skill.name}
                skill={skill}
                installedExecutors={getInstalledExecutors(skill)}
                onClick={() => handleCardClick(skill)}
              />
            ))}
          </div>
        )}
      </Spin>
    );
  })();

  return (
    <div>
      {toolbar}
      {contentArea}

      {/* 详情 Drawer */}
      <Drawer
        title={
          <Space>
            <FileTextOutlined style={{ color: '#7C3AED' }} />
            <span>{selectedSkill?.short_name || '技能详情'}</span>
            {selectedSkill && (
              <Tag color="purple">{selectedSkill.source_meta?.display_name || selectedSkill.source}</Tag>
            )}
          </Space>
        }
        placement="right"
        width={640}
        onClose={() => setDrawerOpen(false)}
        open={drawerOpen}
      >
        {contentLoading ? (
          <div style={{ textAlign: 'center', padding: 40 }}>
            <Spin size="large" />
          </div>
        ) : (
          <div>
            {/* 描述 */}
            {selectedSkill?.description_zh && (
              <Alert
                message={selectedSkill.description_zh}
                type="info"
                showIcon
                icon={<InfoCircleOutlined />}
                style={{ marginBottom: 16 }}
              />
            )}

            {/* 操作按钮 */}
            <div style={{
              display: 'flex',
              gap: 8,
              flexWrap: 'wrap',
              marginBottom: 16,
              padding: '12px',
              background: isDark ? 'rgba(255,255,255,0.04)' : 'rgba(0,0,0,0.02)',
              borderRadius: 8,
              border: `1px solid ${isDark ? 'rgba(255,255,255,0.08)' : 'rgba(0,0,0,0.06)'}`,
            }}>
              <Button
                type="primary"
                icon={<DownloadOutlined />}
                onClick={handleOpenInstall}
              >
                安装
              </Button>
              <Button
                icon={<FolderOutlined />}
                disabled={files.length === 0}
              >
                文件 ({files.length})
              </Button>
            </div>

            {/* 元信息 */}
            <Descriptions bordered size="small" column={2}>
              <Descriptions.Item label="名称" span={2}>
                {selectedSkill?.name}
              </Descriptions.Item>
              <Descriptions.Item label="来源">
                <Tag color="purple">{selectedSkill?.source_meta?.display_name || selectedSkill?.source}</Tag>
              </Descriptions.Item>
              <Descriptions.Item label="版本">
                {selectedSkill?.version || <Text type="secondary">未指定</Text>}
              </Descriptions.Item>
              <Descriptions.Item label="文件数">
                {selectedSkill?.file_count || 0}
              </Descriptions.Item>
              <Descriptions.Item label="大小">
                {formatSize(selectedSkill?.total_size || 0)}
              </Descriptions.Item>
              <Descriptions.Item label="更新时间" span={2}>
                {formatTime(selectedSkill?.modified_at ?? null)}
              </Descriptions.Item>
            </Descriptions>

            {/* 安装状态 */}
            {selectedSkill && getInstalledExecutors(selectedSkill).length > 0 && (
              <div style={{ marginTop: 16 }}>
                <Text type="secondary" style={{ fontSize: 13 }}>已安装到：</Text>
                {getInstalledExecutors(selectedSkill).map(exec => {
                  const opt = EXECUTORS.find(e => e.value === exec);
                  return (
                    <Tag key={exec} color={opt?.color || '#7C3AED'} style={{ marginTop: 4 }}>
                      {opt?.label || exec}
                    </Tag>
                  );
                })}
              </div>
            )}

            {/* SKILL.md 内容预览 */}
            <h3 style={{
              margin: '16px 0 8px',
              color: isDark ? '#e2e8f0' : '#595959',
            }}>SKILL.md 预览</h3>
            <XMarkdown
              content={content}
              escapeRawHtml={true}
              style={{
                fontFamily: 'Fira Code, monospace',
                fontSize: 13,
                background: isDark ? '#1a1a2e' : '#1e1e1e',
                color: '#d4d4d4',
                padding: '12px',
                borderRadius: '8px',
              }}
            />
          </div>
        )}
      </Drawer>

      {/* 安装 Modal */}
      <Modal
        title={
          <Space>
            <DownloadOutlined style={{ color: '#7C3AED' }} />
            <span>安装技能到执行器</span>
          </Space>
        }
        open={installModalOpen}
        onCancel={() => setInstallModalOpen(false)}
        onOk={handleInstall}
        okText={`安装到 ${targetExecutors.length} 个执行器`}
        okButtonProps={{ disabled: targetExecutors.length === 0 }}
        confirmLoading={installing}
        width={480}
      >
        <div style={{ marginBottom: 16 }}>
          <Text type="secondary">
            将 <Text strong>{selectedSkill?.short_name}</Text> 安装到以下执行器：
          </Text>
        </div>
        <Checkbox.Group
          value={targetExecutors}
          onChange={v => setTargetExecutors(v as string[])}
          style={{ width: '100%' }}
        >
          <Row gutter={[8, 8]}>
            {EXECUTORS.map(exec => {
              const installedExecs = selectedSkill ? getInstalledExecutors(selectedSkill) : [];
              const alreadyHas = installedExecs.includes(exec.value);
              return (
                <Col span={12} key={exec.value}>
                  <Checkbox value={exec.value}>
                    <span style={{ display: 'inline-flex', alignItems: 'center', gap: 4 }}>
                      <span style={{
                        width: 6, height: 6, borderRadius: '50%',
                        backgroundColor: exec.color,
                      }} />
                      {exec.label}
                      {alreadyHas && <Tag color="orange" style={{ fontSize: 10 }}>已安装</Tag>}
                    </span>
                  </Checkbox>
                </Col>
              );
            })}
          </Row>
        </Checkbox.Group>
      </Modal>
    </div>
  );
}

// ─────────────────────────────────────────────────────────────────────────────
// 辅助函数：格式化 Stars 数
// ─────────────────────────────────────────────────────────────────────────────
function formatStars(n: number): string {
  if (n >= 10000) {
    return (n / 1000).toFixed(1).replace(/\.0$/, '') + 'k';
  }
  return n.toLocaleString();
}
