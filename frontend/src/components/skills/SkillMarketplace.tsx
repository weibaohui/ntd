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
import * as db from '@/utils/database';
import type { SkillFileInfo } from '@/utils/database/skills';
import { SkillFileBrowserModal } from './SkillFileBrowserModal';
import './SkillMarketplace.css';

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

  // 用透明度叠加的实色变量构造悬停/常态边框，避免 hardcoded rgba；
  // 主题切换时整张卡会一起继承，亮/暗色都不需要再判断 isDark。
  return (
    // 用 div 承载 .market-source-card 类——Ant Card 不转发 className，
    // 但 CSS 选择器需要它才能 hover 抬升/换描边
    <div className="market-source-card" role="button" tabIndex={0}
      onClick={onClick}
      onKeyDown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); onClick(); } }}
    >
    <Card
      size="small"
      hoverable
      style={{
        borderRadius: 'var(--radius-md)',
        cursor: 'pointer',
        transition: 'all var(--transition-base)',
        background: 'var(--color-bg-elevated)',
        borderColor: 'var(--color-border-secondary)',
        boxShadow: 'var(--shadow-sm)',
      }}
      styles={{ body: { padding: compact ? 12 : 16 } }}
    >
      {/* 名称 + Stars */}
      <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 8 }}>
        <span style={{
          fontSize: compact ? 13 : 15,
          fontWeight: 600,
          color: 'var(--color-text)',
          flex: 1,
          overflow: 'hidden',
          textOverflow: 'ellipsis',
          whiteSpace: 'nowrap',
        }}>
          {name}
        </span>
        {(meta?.stars ?? 0) > 0 && (
          // GitHub 星标徽章：自配背景/边框/文字用项目 --color-warning token——
          // 直接用 ant color="warning" 的文本色（暗色 catppuccin 浅黄）在亮色背景上对比度差，
          // 改成显式指定 warning-bg 作底 + warning 作字，亮/暗双主题都清晰
          <Tag style={{
            margin: 0,
            fontSize: 11,
            padding: '0 6px',
            background: 'var(--color-warning-bg)',
            color: 'var(--color-warning)',
            border: '1px solid color-mix(in srgb, var(--color-warning) 40%, transparent)',
          }}>
            <StarOutlined style={{ fontSize: 10 }} /> {formatStars(meta!.stars)}
          </Tag>
        )}
      </div>

      {/* 描述：min-height 兜底——描述即使只有 1 行也撑出与最长同行一致的视觉空间，
         让同行的所有卡片底边对齐 */}
      {meta?.description && (
        <Paragraph
          ellipsis={{ rows: compact ? 2 : 3 }}
          style={{
            fontSize: 12,
            color: 'var(--color-text-secondary)',
            marginBottom: 8,
            lineHeight: 1.5,
            minHeight: compact ? 36 : 54,
          }}
        >
          {meta.description}
        </Paragraph>
      )}

      {/* GitHub + License + 技能数：GitHub 链接改用主色，与设置/Experts 等页面保持一致；
         margin-top:auto 让 footer 靠底，对齐到同行卡片底部——即使描述行数不同，卡片高度一致 */}
      <div style={{
        display: 'flex',
        alignItems: 'center',
        gap: 8,
        flexWrap: 'wrap',
        marginTop: 'auto',
        paddingTop: 8,
      }}>
        {meta?.github_url && (
          <a
            href={meta.github_url}
            target="_blank"
            rel="noopener noreferrer"
            onClick={(e) => e.stopPropagation()}
            style={{
              fontSize: 11,
              color: 'var(--color-primary)',
              display: 'inline-flex',
              alignItems: 'center',
              gap: 4,
            }}
          >
            <LinkOutlined style={{ fontSize: 10 }} /> GitHub
          </a>
        )}
        {meta?.license && (
          // License 标签：统一走 --color-bg-tertiary 底色 + --color-text 文字，
          // 暗色下与正文层次区分，读得清
          <Tag style={{
            margin: 0,
            fontSize: 10,
            lineHeight: '16px',
            padding: '0 6px',
            background: 'var(--color-bg-tertiary)',
            color: 'var(--color-text-secondary)',
            border: 'none',
          }}>
            {meta.license}
          </Tag>
        )}
        <span style={{
          marginLeft: 'auto',
          fontSize: 11,
          color: 'var(--color-text-tertiary)',
        }}>
          {skillCount} 个技能
        </span>
      </div>
    </Card>
    </div>
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
  // 颜色全部走 CSS 变量；不再用硬编码 purple，与项目 cyan 主色一致
  const isInstalled = installedExecutors.length > 0;

  return (
    // 用 div 承载 .market-skill-card 类——Ant Card 不转发 className，
    // 但 CSS 选择器需要它才能应用 hover 抬升和换主色描边
    <div className="market-skill-card" role="button" tabIndex={0}
      aria-label={skill.short_name}
      onClick={onClick}
      onKeyDown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); onClick(); } }}
    >
    <Card
      size="small"
      hoverable
      style={{
        // 与 ExpertCard / TeamCard 同款：常态有 --shadow-sm，避免白底卡贴白底容器看不出边界
        borderRadius: 'var(--radius-md)',
        cursor: 'pointer',
        transition: 'all var(--transition-base)',
        position: 'relative',
        overflow: 'hidden',
        background: 'var(--color-bg-elevated)',
        borderColor: 'var(--color-border-secondary)',
        boxShadow: 'var(--shadow-sm)',
      }}
      styles={{ body: { padding: 16 } }}
    >
      {/* 顶部装饰线：用主色渐变，与 Overview 卡片顶部色带呼应；不再用 purple */}
      <div style={{
        position: 'absolute',
        top: 0, left: 0, right: 0,
        height: 2,
        background: 'linear-gradient(90deg, var(--color-primary), color-mix(in srgb, var(--color-primary) 60%, transparent))',
        opacity: 0.6,
      }} />

      {/* 头部：图标 + 名称 + 来源标签 */}
      <div style={{ display: 'flex', alignItems: 'flex-start', gap: 10 }}>
        <div style={{
          width: 36, height: 36, borderRadius: 10,
          background: 'var(--color-primary-bg)',
          display: 'flex', alignItems: 'center', justifyContent: 'center',
          color: 'var(--color-primary)',
          fontSize: 15, fontWeight: 600, flexShrink: 0,
          border: '1px solid var(--color-primary-light)',
        }}>
          {skill.short_name.charAt(0).toUpperCase()}
        </div>
        <div style={{ flex: 1, minWidth: 0 }}>
          <div style={{
            fontSize: 14, fontWeight: 500,
            color: 'var(--color-text)',
            overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap',
          }}>
            {skill.short_name}
          </div>
          <div style={{
            fontSize: 11,
            color: 'var(--color-text-tertiary)',
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

      {/* 描述：min-height 让短描述的卡也撑出统一高度，对齐同行的卡底 */}
      {(skill.description_zh || skill.description) && (
        <div style={{
          fontSize: 12,
          color: 'var(--color-text-secondary)',
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

      {/* 底部标签：margin-top:auto 让它贴底，跟同行卡片底部对齐 */}
      <div style={{
        display: 'flex', alignItems: 'center', gap: 6,
        marginTop: 'auto', paddingTop: 12, flexWrap: 'wrap',
      }}>
        {skill.version && (
          // 版本徽章统一走主色背景，与头像色调一致
          <Tag style={{
            margin: 0, fontSize: 11, lineHeight: '18px', padding: '0 6px',
            borderRadius: 4, background: 'var(--color-primary-bg)',
            border: 'none', color: 'var(--color-primary)',
          }}>
            v{skill.version}
          </Tag>
        )}
        <span style={{
          marginLeft: 'auto',
          fontSize: 11,
          color: 'var(--color-text-tertiary)',
        }}>
          {formatSize(skill.total_size)}
        </span>
      </div>
    </Card>
    </div>
  );
}

// ─────────────────────────────────────────────────────────────────────────────
// 主组件
// ─────────────────────────────────────────────────────────────────────────────
export function SkillMarketplace() {
  const { message } = App.useApp();
  // 主题色统一走 CSS 变量（var(--color-*)），组件不再判断 isDark；
  // App.css 的 [data-theme="dark"] 会自动切换浅/深色变量取值。

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

  // ── 文件浏览 Modal 状态 ──
  const [fileBrowserOpen, setFileBrowserOpen] = useState(false);

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
    // 用 CSS 变量做背景，主题切换时跟着变；去掉硬编码 rgba 阴影，改用项目 shadow token
    <div style={{
      width: 520,
      maxHeight: 400,
      overflowY: 'auto',
      padding: 12,
      background: 'var(--color-bg-elevated)',
      borderRadius: 'var(--radius-md)',
      boxShadow: 'var(--shadow-lg)',
      border: '1px solid var(--color-border-light)',
    }}>
      <div style={{
        display: 'grid',
        gridTemplateColumns: 'repeat(2, 1fr)',
        gap: 10,
      }}>
        {/* 全部来源选项：
           激活态用更粗主色边框 + 加粗主色标题来标识——
           不动底色，避免在暗色主题下与周边卡产生"陷下去"或"凸出来"的层差。
           边框宽度 2px 配合主色，亮/暗色都清晰可辨。
        */}
        <Card
          size="small"
          hoverable
          onClick={() => { setFilterSource('all'); }}
          style={{
            borderRadius: 'var(--radius-sm)',
            cursor: 'pointer',
            borderColor: filterSource === 'all'
              ? 'var(--color-primary)'
              : 'var(--color-border-secondary)',
            borderWidth: filterSource === 'all' ? 2 : 1,
            borderStyle: 'solid',
            background: 'var(--color-bg-elevated)',
          }}
          styles={{ body: { padding: 12 } }}
        >
          <div style={{
            fontSize: 14,
            fontWeight: filterSource === 'all' ? 600 : 500,
            color: filterSource === 'all' ? 'var(--color-primary)' : 'var(--color-text)',
          }}>全部来源</div>
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

      {/* 搜索框：圆角半径用 radius-xl 与项目其他搜索框保持一致 */}
      <Input
        placeholder="搜索技能..."
        prefix={<SearchOutlined style={{ color: 'var(--color-text-tertiary)' }} />}
        value={searchText}
        onChange={e => setSearchText(e.target.value)}
        style={{ width: 220, borderRadius: 'var(--radius-xl)' }}
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
          {/* 来源信息头：背景与边框统一走 CSS 变量，与 Settings/Experts 等页面信息块风格一致 */}
          <div style={{
            marginBottom: 16,
            padding: '12px 16px',
            background: 'var(--color-bg-card)',
            borderRadius: 'var(--radius-md)',
            border: '1px solid var(--color-border-light)',
          }}>
            <Space wrap>
              <Text strong style={{ fontSize: 15, color: 'var(--color-text)' }}>
                {sourceMeta?.display_name || activeSource}
              </Text>
              {(sourceMeta?.stars ?? 0) > 0 && (
                // 详情头部星标：同 SourceCard 内的 tag，自配 --color-warning 文字与底色
                <Tag style={{
                  background: 'var(--color-warning-bg)',
                  color: 'var(--color-warning)',
                  border: '1px solid color-mix(in srgb, var(--color-warning) 40%, transparent)',
                  fontWeight: 500,
                }}>
                  <StarOutlined /> {formatStars(sourceMeta!.stars)}
                </Tag>
              )}
              {sourceMeta?.license && (
                <Tag style={{
                  background: 'var(--color-bg-tertiary)',
                  color: 'var(--color-text-secondary)',
                  border: 'none',
                }}>
                  {sourceMeta.license}
                </Tag>
              )}
              {sourceMeta?.github_url && (
                <a
                  href={sourceMeta.github_url}
                  target="_blank"
                  rel="noopener noreferrer"
                  style={{ color: 'var(--color-primary)' }}
                >
                  <LinkOutlined /> GitHub
                </a>
              )}
            </Space>
            {sourceMeta?.description && (
              <div style={{
                fontSize: 12,
                color: 'var(--color-text-secondary)',
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

      {/* 详情 Drawer：主色图标/标题色全部走 CSS 变量，与项目其他 Drawer 视觉一致 */}
      <Drawer
        title={
          <Space>
            <FileTextOutlined style={{ color: 'var(--color-primary)' }} />
            <span style={{ color: 'var(--color-text)' }}>{selectedSkill?.short_name || '技能详情'}</span>
            {selectedSkill && (
              <Tag color="cyan">{selectedSkill.source_meta?.display_name || selectedSkill.source}</Tag>
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

            {/* 操作按钮容器：背景/边框统一走变量，去掉 hardcoded rgba */}
            <div style={{
              display: 'flex',
              gap: 8,
              flexWrap: 'wrap',
              marginBottom: 16,
              padding: '12px',
              background: 'var(--color-bg-card)',
              borderRadius: 'var(--radius-sm)',
              border: '1px solid var(--color-border-light)',
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
                onClick={() => setFileBrowserOpen(true)}
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
                <Tag color="cyan">{selectedSkill?.source_meta?.display_name || selectedSkill?.source}</Tag>
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
                    <Tag
                      key={exec}
                      color={opt?.color || 'cyan'}
                      style={{ marginTop: 4 }}
                    >
                      {opt?.label || exec}
                    </Tag>
                  );
                })}
              </div>
            )}

            {/* SKILL.md 内容预览：背景/文字走变量，让暗色主题下也能正常阅读 */}
            <h3 style={{
              margin: '16px 0 8px',
              color: 'var(--color-text)',
            }}>SKILL.md 预览</h3>
            <XMarkdown
              content={content}
              escapeRawHtml={true}
              style={{
                fontFamily: 'var(--font-mono)',
                fontSize: 13,
                background: 'var(--color-bg)',
                color: 'var(--color-text)',
                padding: '12px',
                borderRadius: 'var(--radius-sm)',
                border: '1px solid var(--color-border)',
              }}
            />
          </div>
        )}
      </Drawer>

      {/* 安装 Modal：图标主色走变量，背景交给 Modal 主题 */}
      <Modal
        title={
          <Space>
            <DownloadOutlined style={{ color: 'var(--color-primary)' }} />
            <span style={{ color: 'var(--color-text)' }}>安装技能到执行器</span>
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

      {/* 文件浏览 Modal：复用与已安装技能一致的 SkillFileBrowserModal。
          对 bundled 技能而言，SKILL.md 用已缓存的 content（presetContent），
          其他文件 loadContent 报错 → 预览区显示「无法加载」占位文案。 */}
      <SkillFileBrowserModal
        open={fileBrowserOpen}
        onClose={() => setFileBrowserOpen(false)}
        title={selectedSkill?.name || ''}
        badgeLabel={selectedSkill?.source_meta?.display_name || selectedSkill?.source}
        files={toSkillFileInfos(files)}
        loading={contentLoading}
        presetContent={content}
        presetPath="SKILL.md"
        loadContent={async (file) => {
          if (file.path === 'SKILL.md') {
            return content;
          }
          // bundled API 没提供单文件内容接口 → 让预览区显示「无法加载」占位
          throw new Error('市场暂不支持预览此文件');
        }}
      />
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

// ─────────────────────────────────────────────────────────────────────────────
// 文件浏览：复用 SkillFileBrowserModal。
// bundled API 只返回 SKILL.md 的 content + 文件元信息（path+size），
// 其他文件没有读取接口，所以 loadContent 对 SKILL.md 走缓存，其他文件抛出错误。
// SkillFilePreview 会捕获错误并展示"无法加载"占位。
// ─────────────────────────────────────────────────────────────────────────────
function toSkillFileInfos(files: BundledSkillFile[]): SkillFileInfo[] {
  // 转类型补齐 SkillFileInfo 字段（modified_at 改为可选，这里用空串占位）
  return files.map(f => ({
    path: f.path,
    size: f.size,
    modified_at: '',
  }));
}
