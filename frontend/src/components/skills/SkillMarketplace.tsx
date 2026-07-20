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
import { useState, useEffect, useCallback, useRef } from 'react';
import {
  Card, Tag, Input, Empty, Spin, App,
  Drawer, Descriptions, Button, Space, Modal, Checkbox, Row, Col,
  Alert, Typography, Dropdown, Divider, Pagination,
} from 'antd';
import {
  SearchOutlined, FileTextOutlined, DownloadOutlined,
  InfoCircleOutlined, FolderOutlined, AppstoreOutlined,
  UnorderedListOutlined, ArrowLeftOutlined, StarOutlined,
  LinkOutlined, DownOutlined,
} from '@ant-design/icons';
import XMarkdown from '@ant-design/x-markdown';
import { bundledApi, type BundledSkillMeta, type BundledSkillFile, type SkillSourceMeta, type SkillSourceWithCount } from '@/api/bundled';
import type { ExecutorSkills } from '@/types';
import { EXECUTORS } from '@/types';
import { formatSize, formatTime } from './helpers';
import * as db from '@/utils/database';
import type { SkillFileInfo } from '@/utils/database/skills';
import { SkillFileBrowserModal } from './SkillFileBrowserModal';
import { useIsMobile } from '@/hooks/useIsMobile';
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
      onKeyDown={(e) => {
        // 仅当焦点落在卡片本身（而非内部可聚焦子元素，如 GitHub 链接）时才触发；
        // 否则 Enter 会从子链接冒泡上来被 preventDefault，导致链接反而打不开。
        if (e.currentTarget === e.target && (e.key === 'Enter' || e.key === ' ')) {
          e.preventDefault();
          onClick();
        }
      }}
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
      <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 8, minWidth: 0 }}>
        <span style={{
          fontSize: compact ? 13 : 15,
          fontWeight: 600,
          color: 'var(--color-text)',
          flex: 1,
          // minWidth:0 是关键：flex 子项默认 min-width:auto，
          // 长标题（如「Claude Skills Collection (BbgnsurfTec)」）会撑破列宽，
          // 把它降到 0 后 overflow+ellipsis 才能真正生效
          minWidth: 0,
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
      onKeyDown={(e) => {
        // 仅当焦点落在卡片本身（而非内部可聚焦子元素，如 GitHub 链接）时才触发；
        // 否则 Enter 会从子链接冒泡上来被 preventDefault，导致链接反而打不开。
        if (e.currentTarget === e.target && (e.key === 'Enter' || e.key === ' ')) {
          e.preventDefault();
          onClick();
        }
      }}
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
  // 移动端判定：来源筛选下拉在窄屏改成单列、宽度贴齐视口（见 dropdownContent）。
  const isMobile = useIsMobile();
  const [skills, setSkills] = useState<BundledSkillMeta[]>([]);
  const [sources, setSources] = useState<Record<string, SkillSourceMeta>>({});
  const [loading, setLoading] = useState(false);

  // ── 分页状态 ──
  // 两种视图模式都走后端分页，绝不返回全量数据。
  // 每种模式独立维护 page，避免切换模式时带着旧页码翻到空页。
  // 默认 30 条/页，和桌面卡片网格双列布局下的可读性比较平衡。
  const ALL_SKILLS_PAGE_SIZE = 30;
  // browse-sources 模式下「来源网格」的页码（按来源翻页）
  const [browseSourcesPage, setBrowseSourcesPage] = useState(1);
  // browse-sources 模式下「进入某个来源后的技能列表」页码（按技能翻页）
  const [browseSkillsPage, setBrowseSkillsPage] = useState(1);
  // all-skills 模式的页码
  const [allPage, setAllPage] = useState(1);
  // total 是「过滤后」的技能数（后端先按 source/keyword 过滤再分页），
  // 前端据此渲染 Pagination 组件，而不是直接看当前页的 skills.length。
  const [total, setTotal] = useState(0);
  // 来源分页响应：来源网格专用，与技能分页彻底分离
  const [sourcesList, setSourcesList] = useState<SkillSourceWithCount[]>([]);
  const [sourcesTotal, setSourcesTotal] = useState(0);

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

  // 详情请求竞态守卫：每次点击自增并记下本次序号；晚返回的旧请求若发现序号已变就丢弃结果，
  // 避免快速连点 A→B 时 A 的内容把 B 的详情覆盖掉（旧请求的 finally 也不会误关 B 的 loading）。
  const detailReqIdRef = useRef(0);

  // 列表请求竞态守卫（loadSkills / loadSources 共用）：
  // 翻页 / 切视图 / 改搜索词时旧请求可能晚于新请求返回，
  // 用序号识别「最新」请求，过期的 setState 全部静默丢弃；
  // setLoading(false) 也仅由最新请求触发，避免中途失败的旧请求
  // 把 loading 提前关掉造成 spinner 闪烁。
  const reqGenRef = useRef(0);

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
   *
   * 设计取舍（强制分页）：
   * - 两种视图模式都走后端分页，绝不返回全量数据，避免一次把上千张
   *   技能卡片塞进 DOM 把首屏渲染拖垮。
   * - 来源网格按「来源」独立翻页（loadSources），与技能分页职责分离。
   * - total 是「过滤后」的技能数，前端 Pagination 据此渲染页码。
   *
   * 竞态保护：快速翻页 / 切换视图时，旧的请求可能晚于新请求返回，
   * 若直接 setState 会用旧数据覆盖新数据。这里用 reqGenRef 给每次请求
   * 打序号，仅最新请求的结果（成功 / 失败）能落到 state，
   * 过期请求静默丢弃；setLoading(false) 也只由最新请求触发，
   * 避免「A 失败先把 loading 关掉，但 B 还在路上」的闪烁。
   */
  const loadSkills = useCallback(async () => {
    const myGen = ++reqGenRef.current;
    setLoading(true);
    try {
      // 当前视图模式对应的页码：切换模式时各自独立的 page 互不干扰
      const currentPage = viewMode === 'all-skills' ? allPage : browseSkillsPage;
      // 过滤参数下沉到后端：
      // - 全部技能模式把 filterSource / searchText 作为 source / keyword 传给后端
      // - 按来源浏览模式下「进入某个来源」用 activeSource，来源网格则不带 source
      // 后端先过滤再分页，total 就是过滤后的计数，前端 Pagination 据此渲染。
      const source = viewMode === 'all-skills'
        ? (filterSource === 'all' ? undefined : filterSource)
        : (activeSource ?? undefined);
      const keyword = searchText.trim() || undefined;
      const res = await bundledApi.getSkills({
        page: currentPage,
        page_size: ALL_SKILLS_PAGE_SIZE,
        source,
        keyword,
      });
      // 过期请求：在我之后又发起了新请求，新数据才是用户当前想看的，旧结果直接丢弃
      if (myGen !== reqGenRef.current) return;
      setSkills(res.skills);
      setSources(res.sources);
      setTotal(res.total);
    } catch (e: any) {
      if (myGen !== reqGenRef.current) return;
      message.error('加载技能列表失败: ' + (e?.message || e));
    } finally {
      // 仅最新请求负责关 loading，否则中途失败的过期请求会把 loading 提前关掉
      if (myGen === reqGenRef.current) setLoading(false);
    }
  }, [message, viewMode, allPage, browseSkillsPage, filterSource, activeSource, searchText, ALL_SKILLS_PAGE_SIZE]);

  /**
   * 加载来源分页列表
   *
   * 来源网格专用：按「来源」本身翻页，与技能分页彻底分离。
   * 来源网格的每个 SourceCard 显示 skill_count（过滤前计数），
   * sourcesTotal 是过滤后的来源总数，前端 Pagination 据此渲染。
   *
   * 竞态保护同 loadSkills：复用 reqGenRef，唯一序号、过期丢弃。
   */
  const loadSources = useCallback(async () => {
    const myGen = ++reqGenRef.current;
    setLoading(true);
    try {
      const keyword = searchText.trim() || undefined;
      const res = await bundledApi.getSkillSources({
        page: browseSourcesPage,
        page_size: ALL_SKILLS_PAGE_SIZE,
        keyword,
      });
      // 过期请求直接丢弃，不写 sourcesList / sourcesTotal
      if (myGen !== reqGenRef.current) return;
      setSourcesList(res.sources);
      setSourcesTotal(res.total);
    } catch (e: any) {
      if (myGen !== reqGenRef.current) return;
      message.error('加载来源列表失败: ' + (e?.message || e));
    } finally {
      if (myGen === reqGenRef.current) setLoading(false);
    }
  }, [message, browseSourcesPage, searchText, ALL_SKILLS_PAGE_SIZE]);

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
    // 来源网格走 loadSources（按来源翻页），其余技能列表场景走 loadSkills
    if (viewMode === 'browse-sources' && !activeSource) {
      loadSources();
    } else {
      loadSkills();
    }
    loadInstalled();
  }, [viewMode, activeSource, loadSkills, loadSources, loadInstalled]);

  // 过滤已下沉到后端（loadSkills 下发 source / keyword）：
  // 后端先过滤再分页，返回的 skills 就是当前页的过滤后切片，
  // 这里直接用 skills，不再做本地二次过滤，避免与后端切片叠加导致分页错位。

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
    // 先占坑：立即清空旧内容并打开 Drawer，让用户感到响应即时；
    // 真正的内容等异步返回、且确认仍是最新请求后才写入。
    const reqId = ++detailReqIdRef.current;
    setSelectedSkill(skill);
    setDrawerOpen(true);
    setContent('');
    setFiles([]);
    setContentLoading(true);
    try {
      const res = await bundledApi.getSkillContent(skill.name);
      // 序号已变 → 等待期间用户又点了别的技能，丢弃这次过期结果，不覆盖新选中技能。
      if (reqId !== detailReqIdRef.current) return;
      setContent(res.content);
      setFiles(res.files);
    } catch {
      if (reqId !== detailReqIdRef.current) return;
      setContent('加载内容失败');
    } finally {
      // 只关「最新那次」请求的 loading；过期请求的 loading 由接管它的新请求自己管理。
      if (reqId === detailReqIdRef.current) setContentLoading(false);
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
    // 切回来源浏览时重置页码，避免带着「全部技能」模式的 page 状态回来
    setBrowseSourcesPage(1);
    setBrowseSkillsPage(1);
  };

  const switchToAllSkills = () => {
    setViewMode('all-skills');
    setActiveSource(null);
    setFilterSource('all');
    setSearchText('');
    // 进入「全部技能」分页模式，始终从第 1 页开始
    setAllPage(1);
  };

  const enterSource = (sourceKey: string) => {
    setActiveSource(sourceKey);
    setSearchText('');
  };

  // ── 下拉筛选内容 ──
  const dropdownContent = (
    // 用 CSS 变量做背景，主题切换时跟着变；去掉硬编码 rgba 阴影，改用项目 shadow token
    <div style={{
      // 移动端：弹层贴近视口宽度，避免 520px 固定宽在窄屏溢出；桌面端保持 520px。
      width: isMobile ? 'calc(100vw - 32px)' : 520,
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
        // 移动端单列「一行一个来源」；桌面端两列。垂直滚动由外层 maxHeight + overflowY 提供。
        gridTemplateColumns: isMobile ? '1fr' : 'repeat(2, 1fr)',
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
          onClick={() => { setFilterSource('all'); setAllPage(1); }}
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
            {total} 个技能
          </div>
        </Card>

        {/* 下拉来源列表：复用 sourcesList（来源网格分页响应），
            它已经按来源名排序、含 skill_count */}
        {sourcesList.map(src => (
          <SourceCard
            key={src.meta.name}
            sourceKey={src.meta.name}
            meta={src.meta}
            skillCount={src.skill_count}
            onClick={() => { setFilterSource(src.meta.name); setAllPage(1); }}
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
        onChange={e => {
          setSearchText(e.target.value);
          // 搜索会改变后端过滤结果，重置回第 1 页避免停留在空页
          if (viewMode === 'all-skills') setAllPage(1);
        }}
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
          ? `${sourcesTotal} 个来源`
          : `${viewMode === 'browse-sources' ? skills.length : skills.length} 个技能`
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
      if (sourcesList.length === 0) {
        return (
          <div style={{ textAlign: 'center', padding: '60px 20px' }}>
            <Empty description={searchText ? '无匹配来源' : '暂无技能来源'} />
          </div>
        );
      }
      return (
        <div>
          <div style={{
            display: 'grid',
            gridTemplateColumns: 'repeat(auto-fill, minmax(300px, 1fr))',
            gap: 16,
          }}>
            {/* 来源网格直接用后端返回的 sourcesList（已按来源分页），
                每个 SourceCard 显示 skill_count（过滤前计数） */}
            {sourcesList.map(src => (
              <SourceCard
                key={src.meta.name}
                sourceKey={src.meta.name}
                meta={src.meta}
                skillCount={src.skill_count}
                onClick={() => enterSource(src.meta.name)}
              />
            ))}
          </div>
          {/* 来源网格分页器：sourcesTotal 是过滤后的来源总数，
              browseSourcesPage 翻页时 loadSources 重拉当前页来源 */}
          {sourcesTotal > 0 && (
            <div style={{
              display: 'flex',
              justifyContent: 'center',
              marginTop: 24,
            }}>
              <Pagination
                current={browseSourcesPage}
                pageSize={ALL_SKILLS_PAGE_SIZE}
                total={sourcesTotal}
                onChange={(nextPage) => setBrowseSourcesPage(nextPage)}
                showSizeChanger={false}
                showTotal={(count) => `共 ${count} 个来源`}
              />
            </div>
          )}
        </div>
      );
    }

    // 按来源浏览 → 某个来源的技能列表
    if (viewMode === 'browse-sources' && activeSource) {
      // skills 已经是 loadSkills 按 activeSource 过滤后当前页的切片
      const sourceSkills = skills;
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

          {/* 单个来源的技能列表分页器：用 browseSkillsPage 翻页，
              loadSkills 在该模式下按 activeSource 过滤后分页拉取技能 */}
          {total > 0 && (
            <div style={{
              display: 'flex',
              justifyContent: 'center',
              marginTop: 24,
            }}>
              <Pagination
                current={browseSkillsPage}
                pageSize={ALL_SKILLS_PAGE_SIZE}
                total={total}
                onChange={(nextPage) => setBrowseSkillsPage(nextPage)}
                showSizeChanger={false}
                showTotal={(count) => `共 ${count} 个技能`}
              />
            </div>
          )}
        </div>
      );
    }

    // 全部技能模式
    // 该模式走后端分页：loadSkills 只拉当前页的 ALL_SKILLS_PAGE_SIZE 条，
    // 底部 Pagination 翻页时通过 page 状态触发 loadSkills 重跑。
    // 注意 total 是「过滤后」的技能数，前端据此渲染页码而不是看当前页 length。
    return (
      <Spin spinning={loading}>
        {skills.length === 0 ? (
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
            {skills.map(skill => (
              <MarketSkillCard
                key={skill.name}
                skill={skill}
                installedExecutors={getInstalledExecutors(skill)}
                onClick={() => handleCardClick(skill)}
              />
            ))}
          </div>
        )}
        {/* 分页器：仅在「全部技能」模式显示，total 由后端在分页切片前写入 */}
        {viewMode === 'all-skills' && total > 0 && (
          <div style={{
            display: 'flex',
            justifyContent: 'center',
            marginTop: 24,
          }}>
            <Pagination
              current={allPage}
              pageSize={ALL_SKILLS_PAGE_SIZE}
              total={total}
              onChange={(nextPage) => setAllPage(nextPage)}
              showSizeChanger={false}
              showTotal={(count) => `共 ${count} 个技能`}
            />
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
          SKILL.md 走已缓存的 content（presetContent，省一次请求），
          其他文件调 bundledApi.getSkillFileContent 实时读取后端文件内容。 */}
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
          // SKILL.md 命中预设缓存，直接返回，避免重复请求
          if (file.path === 'SKILL.md') {
            return content;
          }
          // 没有选中技能时不发请求：name 为空时后端会把 skill_dir 解析成 skills 根目录，
          // 即便服务端已校验 name，前端也应在源头短路，避免发出无意义/异常的请求
          const skillName = selectedSkill?.name;
          if (!skillName) {
            throw new Error('未选中技能，无法读取文件');
          }
          // 其他文件由后端按技能名 + 相对路径读取
          const res = await bundledApi.getSkillFileContent(skillName, file.path);
          return res.content;
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
// bundled 的 content 接口返回 SKILL.md 文本 + 全量文件元信息（path+size）；
// 单文件内容由 GET /api/bundled/skills/{name}/file 按需读取，
// 所以 loadContent 对 SKILL.md 走缓存，其他文件调 getSkillFileContent。
// ─────────────────────────────────────────────────────────────────────────────
function toSkillFileInfos(files: BundledSkillFile[]): SkillFileInfo[] {
  // 转类型补齐 SkillFileInfo 字段（modified_at 改为可选，这里用空串占位）
  return files.map(f => ({
    path: f.path,
    size: f.size,
    modified_at: '',
  }));
}
