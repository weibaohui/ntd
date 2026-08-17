/**
 * useSkillCatalog — 技能市场目录（列表/分页/视图/筛选）数据族 hook（096-W4-4 产物）。
 *
 * 承接原 SkillMarketplace 主组件的目录区块：
 * - 数据族（skills/sources/loading + sourcesList/sourcesTotal）
 * - 分页族（三种视图各自独立页码）
 * - 视图族（viewMode/activeSource）与筛选族（searchText/filterSource）
 * - 两个 loader（loadSkills/loadSources，共用 reqGenRef 序号竞态守卫）
 * - 三个视图切换 action（switchToSourceBrowse / switchToAllSkills / enterSource）——
 *   「切换时联动重置哪些状态」的编排收口于此，新增视图模式只改一处。
 *   历史 bug（切视图漏重置页码导致空白页）即这类联动散落所致，收口后由单测锁定。
 *
 * 函数体逐字搬自主组件原实现，行为等价。
 */

import { useCallback, useEffect, useRef, useState } from 'react';
import { App } from 'antd';
import {
  bundledApi,
  type BundledSkillMeta,
  type SkillSourceMeta,
  type SkillSourceWithCount,
} from '@/api/bundled';

type ViewMode = 'browse-sources' | 'all-skills';

/** 两种视图模式都走后端分页，默认 30 条/页（桌面卡片网格双列布局下的可读性平衡点）。
 * 导出供主组件 Pagination 的 pageSize 使用（单一事实源，避免两处写死漂移）。 */
export const ALL_SKILLS_PAGE_SIZE = 30;

export interface SkillCatalogState {
  skills: BundledSkillMeta[];
  sources: Record<string, SkillSourceMeta>;
  loading: boolean;
  total: number;
  sourcesList: SkillSourceWithCount[];
  sourcesTotal: number;
  viewMode: ViewMode;
  activeSource: string | null;
  searchText: string;
  filterSource: string;
  browseSourcesPage: number;
  browseSkillsPage: number;
  allPage: number;
  setSearchText: (v: string) => void;
  setFilterSource: (v: string) => void;
  setBrowseSourcesPage: (v: number) => void;
  setBrowseSkillsPage: (v: number) => void;
  setAllPage: (v: number) => void;
  /** 切到「按来源浏览」视图（重置 activeSource/搜索词/两个浏览页码） */
  switchToSourceBrowse: () => void;
  /** 切到「全部技能」视图（重置 activeSource/来源筛选/搜索词/页码） */
  switchToAllSkills: () => void;
  /** 进入某个来源（重置技能页码与搜索词） */
  enterSource: (sourceKey: string) => void;
  /** 返回来源网格（重置 activeSource 与两个浏览页码） */
  backToSourceGrid: () => void;
}

export function useSkillCatalog(): SkillCatalogState {
  const { message } = App.useApp();

  // ── 数据状态 ──
  const [skills, setSkills] = useState<BundledSkillMeta[]>([]);
  const [sources, setSources] = useState<Record<string, SkillSourceMeta>>({});
  const [loading, setLoading] = useState(false);

  // ── 分页状态 ──
  // 两种视图模式都走后端分页，绝不返回全量数据。
  // 每种模式独立维护 page，避免切换模式时带着旧页码翻到空页。
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

  // 列表请求竞态守卫（loadSkills / loadSources 共用）：
  // 翻页 / 切视图 / 改搜索词时旧请求可能晚于新请求返回，
  // 用序号识别「最新」请求，过期的 setState 全部静默丢弃；
  // setLoading(false) 也仅由最新请求触发，避免中途失败的旧请求
  // 把 loading 提前关掉造成 spinner 闪烁。
  const reqGenRef = useRef(0);

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
    // 抢占本次序号：先 ++ 再读，这样即使同步重入也能保证唯一且递增
    const myGen = ++reqGenRef.current;
    // 进入 loading 状态要早于 await，否则用户在「点完到转圈」之间会有几十毫秒空窗
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
      // keyword 必须 trim：用户粘贴的 "Claude " 和 "Claude" 应当等价，否则搜索结果莫名其妙地变少
      const keyword = searchText.trim() || undefined;
      const res = await bundledApi.getSkills({
        page: currentPage,
        page_size: ALL_SKILLS_PAGE_SIZE,
        source,
        keyword,
      });
      // 过期请求：在我之后又发起了新请求，新数据才是用户当前想看的，旧结果直接丢弃
      if (myGen !== reqGenRef.current) return;
      // 三个 setter 顺序固定为「列表 → 分类 → 总数」，方便阅读；
      // 不打 batch 是因为这些都是原始 useState，独立更新没有性能问题
      setSkills(res.skills);
      setSources(res.sources);
      setTotal(res.total);
    } catch (e: unknown) {
      // 过期请求的错误信息也不展示：用户看到的是「上一次」的错误，已经不准确
      if (myGen !== reqGenRef.current) return;
      message.error('加载技能列表失败: ' + (e instanceof Error ? e.message : String(e)));
    } finally {
      // 仅最新请求负责关 loading，否则中途失败的过期请求会把 loading 提前关掉
      if (myGen === reqGenRef.current) setLoading(false);
    }
  }, [message, viewMode, allPage, browseSkillsPage, filterSource, activeSource, searchText]);

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
    // 与 loadSkills 共用 reqGenRef：保证两类请求的「最新」语义统一，
    // 即用户在「全部技能」模式和「来源网格」模式之间快速切换时，
    // 也只有最新一次请求的结果能落到 state
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
      // 来源网格只更新这两个 state；skills / sources / total 不会被本次调用影响，
      // 避免「来源网格数据」误覆盖「技能列表」数据
      setSourcesList(res.sources);
      setSourcesTotal(res.total);
    } catch (e: unknown) {
      if (myGen !== reqGenRef.current) return;
      message.error('加载来源列表失败: ' + (e instanceof Error ? e.message : String(e)));
    } finally {
      if (myGen === reqGenRef.current) setLoading(false);
    }
  }, [message, browseSourcesPage, searchText]);

  useEffect(() => {
    // 来源网格走 loadSources（按来源翻页），其余技能列表场景走 loadSkills
    if (viewMode === 'browse-sources' && !activeSource) {
      loadSources();
    } else {
      loadSkills();
    }
  }, [viewMode, activeSource, loadSkills, loadSources]);

  // ── 切换视图（联动重置编排收口：历史「漏重置页码」bug 即联动散落所致）──
  const switchToSourceBrowse = useCallback(() => {
    setViewMode('browse-sources');
    setActiveSource(null);
    setSearchText('');
    // 切回来源浏览时重置页码，避免带着「全部技能」模式的 page 状态回来
    setBrowseSourcesPage(1);
    setBrowseSkillsPage(1);
  }, []);

  const switchToAllSkills = useCallback(() => {
    setViewMode('all-skills');
    setActiveSource(null);
    setFilterSource('all');
    setSearchText('');
    // 进入「全部技能」分页模式，始终从第 1 页开始
    setAllPage(1);
  }, []);

  /** 返回来源网格（「返回来源列表」按钮）：activeSource 清空 + 两个浏览页码重置。
   *  原为主组件 JSX 内联的三 setter 联动——与三 switch 同族，一并收口。 */
  const backToSourceGrid = useCallback(() => {
    setActiveSource(null);
    // browseSkillsPage 是「某来源内的技能列表」页码，回到来源网格后这个状态不再有意义
    setBrowseSkillsPage(1);
    // browseSourcesPage 也重置，避免「来源网格 page=3」和「点回刚看的来源 page=1」语义错乱
    setBrowseSourcesPage(1);
  }, []);

  const enterSource = useCallback((sourceKey: string) => {
    // 进入新来源时强制把页码重置回第 1 页：
    // 不同来源的技能数差异很大（旧来源可能翻到第 10 页，新来源总共只有 2 页），
    // 若不重置，用户会卡在「当前页超出新来源总页数 → 显示空白 + Pagination 翻不动」的鬼畜状态。
    setActiveSource(sourceKey);
    setBrowseSkillsPage(1);
    setSearchText('');
  }, []);

  return {
    skills,
    sources,
    loading,
    total,
    sourcesList,
    sourcesTotal,
    viewMode,
    activeSource,
    searchText,
    filterSource,
    browseSourcesPage,
    browseSkillsPage,
    allPage,
    setSearchText,
    setFilterSource,
    setBrowseSourcesPage,
    setBrowseSkillsPage,
    setAllPage,
    switchToSourceBrowse,
    switchToAllSkills,
    enterSource,
    backToSourceGrid,
  };
}
