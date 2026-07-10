/**
 * BlackboardPage — 黑板 Wiki 页面。
 *
 * Wiki 化后的黑板：左侧页面目录树，右侧 Markdown 内容区。
 * 页面分为 index（目录）、topic（主题）、log（日志）三类。
 *
 * 布局（桌面端 ≥768px）：
 *   ┌───────────────────────────────────────────┐
 *   │ 黑板         [倒计时进度条]  [设置] [刷新]   │
 *   ├──────────┬────────────────────────────────┤
 *   │ 目录树    │        Markdown 内容区          │
 *   │  220px   │          flex: 1               │
 *   └──────────┴────────────────────────────────┘
 *
 * 布局（移动端 <768px）：
 *   ┌──────────────────────────┐
 *   │ 黑板   [设置] [刷新]     │
 *   │ [目录按钮]               │  ← 点击打开 Drawer
 *   ├──────────────────────────┤
 *   │     Markdown 内容区       │
 *   │       全宽                │
 *   └──────────────────────────┘
 */

import { useState, useEffect, useCallback, useMemo, useRef } from 'react';
import { Button, Skeleton, message, Modal, Form, InputNumber, Space, Progress, Input, Tabs, Menu, Drawer, Switch } from 'antd';
import { ReloadOutlined, SettingOutlined, UnorderedListOutlined, MenuOutlined, DeleteOutlined } from '@ant-design/icons';
import { PageCard } from '@/components/common/PageCard';
import { TfiBlackboard } from 'react-icons/tfi';
import { XMarkdown } from '@ant-design/x-markdown';
import { useTheme } from '@/hooks/useTheme';
import { useViewState } from '@/hooks/useViewState';
import { useIsMobile } from '@/hooks/useIsMobile';
import type { BlackboardDebounceStatus } from '@/hooks/useExecutionEvents';
import { updateBlackboardConfig, getBlackboard, deleteWikiFile } from '@/utils/database/blackboard';
import { normalizeBlackboardMarkdown } from '@/utils/markdown';
import { ProposalButton } from '@/components/blackboard-proposal/ProposalButton';

/** 黑板 API 返回的配置形状（与后端 BlackboardResponse 对应，不含内容） */
interface BlackboardData {
  id: number;
  workspace_id: number;
  updated_at: string | null;
  /** 黑板更新防抖周期（秒）*/
  blackboard_debounce_secs: number;
  /** 黑板更新防抖条数阈值 */
  blackboard_debounce_count: number;
  /** Wiki 更新提示词模板（单阶段） */
  wiki_prompt: string;
  /** Wiki 对话使用的执行器名称，空/undefined 表示使用默认值 claudecode */
  wiki_chat_executor?: string | null;
  /** Wiki 执行超时（秒），控制 Wiki 任务与 Wiki 对话的最长存活时间 */
  wiki_timeout_secs: number;
  /** 黑板功能总开关 */
  enabled: boolean;
}

/** Wiki 文件列表项（对应后端 WikiFileItem） */
interface WikiFileItem {
  slug: string;
  file_type: 'index' | 'topic' | 'log' | string;
}

/** Wiki 文件内容（对应后端 WikiFileContent） */
interface WikiFileContent {
  slug: string;
  content: string;
}

/** ntd://todo/{id} 协议的前缀，用于解析 LLM 注入的内部链接 */
const NTD_TODO_PROTOCOL_PREFIX = 'ntd://todo/';

/** URL search 参数 `workspace` 的键名 */
const URL_WORKSPACE_PARAM = 'workspace';

/** 默认工作空间 ID（首屏兜底，避免 URL 未带参时无 workspace） */
const DEFAULT_WORKSPACE_ID = 1;

/**
 * Wiki 提示词默认值（单阶段）：与后端 `build_wiki_prompt()` 内置模板保持一致。
 *
 * ⚠️ 注意：此为前端副本，后端 `backend/src/services/blackboard.rs` 的
 * `build_wiki_prompt()` 函数中也有一份，修改时需同步更新两处。
 * 用于在 UI 上展示默认提示词内容，以及"恢复默认"时回填。
 */
const DEFAULT_WIKI_PROMPT = `你是一个工作空间黑板维护者。你的任务是分析新的执行记录，更新 Wiki 页面。

你拥有以下工具，可以直接在执行过程中调用：
- \`ls ~/.ntd/workspace/{{workspace_id}}/wiki/topics/\`：列出现有主题页面
- \`cat ~/.ntd/workspace/{{workspace_id}}/wiki/topics/<slug>.md\`：读取页面内容
- \`ntd todo execution get <id>\`：获取指定执行记录的完整结论（result 字段）

待分析的执行记录 ID 列表：
{{pending_record_ids}}

请按以下步骤操作：
1. 列出现有主题页面，了解当前 Wiki 结构
2. 逐个调用 \`ntd todo execution get <id>\` 获取每条执行记录的结论
3. 分析每条结论涉及哪些主题领域
4. 对于新主题：创建 \`~/.ntd/workspace/{{workspace_id}}/wiki/topics/<slug>.md\`
5. 对于已有主题：编辑文件，追加/更新结论（保持已有内容）
6. 每个页面结构：
   - # 标题（中文）
   - ## 已确认
   - ## 新发现
   - ## 待解决问题
   - ## 矛盾/风险
   - ## 下一步建议
7. 每条结论标注来源，使用 \`ntd todo execution get <record_id>\` 返回结果中的 \`todo_id\` 和 \`id\` 字段，
   生成 app 内链接：(来源: [record_{record_id}](/#/items?id={todo_id}&panel=post&record={record_id}))

完成后输出简短确认即可，无需输出 YAML/JSON。`;


/** Markdown 链接组件 props 形状（XMarkdown ComponentProps 简化版） */
interface MarkdownLinkProps extends React.AnchorHTMLAttributes<HTMLAnchorElement> {
  href?: string;
  children?: React.ReactNode;
}

/**
 * Wiki 文件相对路径的正则：
 * - ./slug           — 直接写 slug，如 ./auth-module
 * - topics/slug.md  — 后端 index 自动生成的格式，如 topics/auth-module.md
 * 用于在 Markdown 内容中点击跳转到对应的 Wiki 页面。
 */
const WIKI_RELATIVE_PATH_REGEX = /^(?:\.\/)?(?:topics\/)?(.+?)(?:\.md)?$/;

/**
 * Markdown 链接渲染器：识别内部链接协议与路径。
 *
 * 行为：
 * - href 以 ntd://todo/ 开头 → 渲染为可点击的"内链"按钮，
 *   点击时通过 useViewState.selectTodo 导航到事项详情，
 *   阻止浏览器尝试解析 ntd:// 自定义协议导致"找不到应用"提示。
 * - href 匹配 ./slug 或 topics/slug.md 格式（Wiki 相对路径）→ 通过 selectWiki 跳转到 Wiki 视图
 * - href 以 / 开头（app 内相对路径，如 /#/items?id=16&panel=post&record=6513）
 *   → 新标签页打开，让用户同时保留 wiki 页面和查看源记录。
 * - 其他 href（http/https/mailto 等）→ 新窗口打开 + rel=noopener 防 tabnabbing。
 */
function TodoLink(props: MarkdownLinkProps & { workspaceId?: number; selectWiki?: (workspaceId: number, slug: string) => void }): React.ReactElement {
  // 用 hook 不能放在条件分支里：TodoLink 总是组件实例，调用安全
  const { selectTodo, selectWiki } = useViewState();
  const { workspaceId, selectWiki: selectWikiProp, ...restProps } = props;
  const href = restProps.href ?? '';

  // 解析 ntd://todo/{id} → 提取纯数字 id
  const isInternal = href.startsWith(NTD_TODO_PROTOCOL_PREFIX);
  const todoId = isInternal ? Number(href.slice(NTD_TODO_PROTOCOL_PREFIX.length)) : NaN;

  // 内部链接：用 button 风格 + onClick，避免浏览器把 ntd:// 解释成未知协议
  if (isInternal && Number.isFinite(todoId)) {
    return (
      <a
        {...restProps}
        href={`#/items?id=${todoId}`}
        // preventDefault：阻止浏览器实际跳到 #/items?id=...，完全交给 selectTodo
        onClick={(e) => {
          e.preventDefault();
          // stopPropagation：避免外层 XMarkdown 的 link 行为再次触发
          e.stopPropagation();
          selectTodo(todoId);
        }}
        style={{ color: 'var(--color-primary, #1677ff)', textDecoration: 'underline', cursor: 'pointer' }}
      >
        {restProps.children}
      </a>
    );
  }

  // Wiki 相对路径：./slug 或 ./topics/slug
  const wikiMatch = href.match(WIKI_RELATIVE_PATH_REGEX);
  if (wikiMatch && workspaceId) {
    const targetSlug = wikiMatch[1];
    // 去掉 .md 后缀（如果有的话）
    const cleanSlug = targetSlug.replace(/\.md$/, '');
    return (
      <a
        {...restProps}
        href={`#/wiki?workspace=${workspaceId}&slug=${encodeURIComponent(cleanSlug)}`}
        onClick={(e) => {
          e.preventDefault();
          e.stopPropagation();
          // 优先使用传入的 selectWiki prop，其次使用 hook 的 selectWiki
          const navigate = selectWikiProp ?? selectWiki;
          if (navigate) {
            navigate(workspaceId, cleanSlug);
          }
        }}
        style={{ color: 'var(--color-primary, #1677ff)', textDecoration: 'underline', cursor: 'pointer' }}
      >
        {restProps.children}
      </a>
    );
  }

  // 内部相对路径（以 / 但非 // 开头，如 /#/items?id=16&panel=post&record=6513）
  // → 新标签页打开，让用户同时保留当前 wiki 页面和查看源记录
  // 排除 // 协议相对 URL，避免把外站链接当作 app 内路径
  if (href.startsWith('/') && !href.startsWith('//')) {
    return (
      <a {...restProps} href={href} target="_blank" rel="noopener noreferrer">
        {restProps.children}
      </a>
    );
  }

  // 外部链接：新窗口打开 + rel=noopener 防 tabnabbing
  return (
    <a {...restProps} target="_blank" rel="noopener noreferrer">
      {restProps.children}
    </a>
  );
}

/** 从 URL ?workspace=N 解析工作空间 ID；解析失败时返回默认值 */
function resolveWorkspaceFromUrl(): number {
  // 在浏览器外（如 SSR/测试）调用 window 会炸；外层先保证只在浏览器跑
  // 从 hash 路由中解析 workspace 参数（hash 格式：#/view?param=value）
  const hash = window.location.hash || '';
  const hashWithoutHash = hash.startsWith('#') ? hash.slice(1) : hash;
  const [, search] = hashWithoutHash.split('?', 2);
  const raw = new URLSearchParams(search || '').get(URL_WORKSPACE_PARAM);
  const parsed = raw ? Number(raw) : NaN;
  return Number.isFinite(parsed) ? parsed : DEFAULT_WORKSPACE_ID;
}

/** 决定当前生效的 workspaceId：prop 优先，否则从 URL 解析 */
function useEffectiveWorkspaceId(propWorkspaceId: number | null | undefined): number {
  // 每次渲染都重新计算：避免 useState 初始化只跑一次的旧 bug
  // 切换工作空间时 propWorkspaceId 会变，依赖它让派生值自动跟随
  return useMemo(() => {
    if (propWorkspaceId != null) return propWorkspaceId;
    return resolveWorkspaceFromUrl();
  }, [propWorkspaceId]);
}

/** 拉取黑板内容的纯函数，便于测试与复用（旧版单文件接口，保留兼容） */
async function fetchBlackboardData(workspaceId: number): Promise<BlackboardData> {
  const res = await fetch(`/api/workspaces/${workspaceId}/blackboard`);
  if (!res.ok) {
    throw new Error(`HTTP ${res.status}`);
  }
  const json = (await res.json()) as { data?: BlackboardData };
  if (!json.data) {
    throw new Error('Empty response body');
  }
  return json.data;
}

/** 拉取单个 Wiki 文件内容 */
async function fetchWikiFileContent(workspaceId: number, slug: string): Promise<WikiFileContent> {
  const res = await fetch(`/api/workspaces/${workspaceId}/wiki/files/${encodeURIComponent(slug)}`);
  if (!res.ok) {
    throw new Error(`HTTP ${res.status}`);
  }
  const json = (await res.json()) as { data?: WikiFileContent };
  if (!json.data) {
    throw new Error('Empty response body');
  }
  return json.data;
}

/** 拉取 Wiki 文件列表 */
async function fetchWikiFiles(workspaceId: number): Promise<WikiFileItem[]> {
  const res = await fetch(`/api/workspaces/${workspaceId}/wiki/files`);
  if (!res.ok) {
    throw new Error(`HTTP ${res.status}`);
  }
  const json = (await res.json()) as { data?: WikiFileItem[] };
  return json.data ?? [];
}



export function BlackboardPage({ workspaceId: propWorkspaceId }: { workspaceId?: number | null }) {
  // 主题：决定黑板容器背景与文字色
  const { themeMode } = useTheme();
  const isDark = themeMode === 'dark';
  // 派生值（不再 useState）：切换工作空间时自动跟随 prop 变化
  const workspaceId = useEffectiveWorkspaceId(propWorkspaceId);
  // 移动端检测
  const isMobile = useIsMobile();
  // 用于同步 URL
  const { replaceUrl, blackboardFile } = useViewState();

  // Wiki 化数据状态
  const [files, setFiles] = useState<WikiFileItem[]>([]);
  const [currentFile, setCurrentFile] = useState<WikiFileContent | null>(null);
  const [currentSlug, setCurrentSlug] = useState<string>('');
  const [filesLoading, setFilesLoading] = useState(true);
  const [fileLoading, setFileLoading] = useState(false);
  // 旧版数据（配置用）
  const [configData, setConfigData] = useStateBlackboardData();

  // 防切换竞态：ref 始终持有「最新一次渲染」的 workspaceId / currentSlug。
  // fetch 回调在 await 前捕获闭包里的旧 key，resolve 后与 ref 比较——
  // 不一致说明期间已切换工作空间/文件，晚到的响应直接丢弃，避免覆盖新工作空间的数据。
  // 与 useLoopExecutions/useExecutionHistory 的 cancelledRef 思路一致（latest-wins）。
  const latestWorkspaceIdRef = useRef(workspaceId);
  latestWorkspaceIdRef.current = workspaceId;
  const latestSlugRef = useRef(currentSlug);
  latestSlugRef.current = currentSlug;
  // 设置弹窗状态
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [settingsSaving, setSettingsSaving] = useState(false);
  const [debounceSecs, setDebounceSecs] = useState<number | null>(600);
  const [debounceCount, setDebounceCount] = useState<number | null>(10);
  const [wikiPrompt, setWikiPrompt] = useState<string>('');
  // Wiki 执行超时（秒）：与后端 DEFAULT_WIKI_TIMEOUT_SECS=300 一致，清空时回退默认
  const [wikiTimeoutSecs, setWikiTimeoutSecs] = useState<number | null>(300);
  const [bbEnabled, setBbEnabled] = useState<boolean>(true);
  const [activeTab, setActiveTab] = useState<'debounce' | 'prompt'>('debounce');
  // 移动端目录 Drawer 开关状态
  const [menuDrawerOpen, setMenuDrawerOpen] = useState(false);

  /**
   * 打开设置弹窗：从已加载的黑板数据中读取 per-workspace 配置。
   * 配置现在由 GET /api/workspaces/{workspaceId}/blackboard 接口随内容一并返回，
   * 不再需要单独调用 db.getConfig()（getConfig 是全局配置，与黑板配置无关）。
   */
  const handleOpenSettings = useCallback(() => {
    if (configData) {
      setDebounceSecs(configData.blackboard_debounce_secs ?? 600);
      setDebounceCount(configData.blackboard_debounce_count ?? 10);
      setWikiPrompt(configData.wiki_prompt ?? '');
      setWikiTimeoutSecs(configData.wiki_timeout_secs ?? 300);
      setBbEnabled(configData.enabled ?? true);
    } else {
      setDebounceSecs(600);
      setDebounceCount(10);
      setWikiPrompt('');
      setWikiTimeoutSecs(300);
      setBbEnabled(true);
    }
    setActiveTab('debounce');
    setSettingsOpen(true);
  }, [configData]);

  // 保存设置
  const handleSaveSettings = useCallback(async () => {
    setSettingsSaving(true);
    try {
      await updateBlackboardConfig(workspaceId, {
        // 用户清空输入时 null → 用默认值，避免后端意外覆盖
        blackboard_debounce_secs: debounceSecs ?? 600,
        blackboard_debounce_count: debounceCount ?? 10,
        wiki_prompt: wikiPrompt,
        wiki_timeout_secs: wikiTimeoutSecs ?? 300,
        enabled: bbEnabled,
      });
      // 保存成功后同步更新 data，避免下次打开弹窗读到旧值
      if (configData) {
        setConfigData({
          ...configData,
          blackboard_debounce_secs: debounceSecs ?? 600,
          blackboard_debounce_count: debounceCount ?? 10,
          wiki_prompt: wikiPrompt,
          wiki_timeout_secs: wikiTimeoutSecs ?? 300,
          enabled: bbEnabled,
        });
      }
      message.success('设置已保存');
      setSettingsOpen(false);
    } catch (err) {
      message.error('保存失败: ' + (err instanceof Error ? err.message : String(err)));
    } finally {
      setSettingsSaving(false);
    }
  }, [workspaceId, debounceSecs, debounceCount, wikiPrompt, wikiTimeoutSecs, bbEnabled, configData]);

  // 恢复默认提示词：把 wikiPrompt 设为内置默认值。
  // 区别于"留空"的语义——留空表示后端使用内置默认；填入默认值表示用户显式采用内置模板。
  const handleRestorePrompt = useCallback(() => {
    setWikiPrompt(DEFAULT_WIKI_PROMPT);
  }, []);

  // 拉取页面列表
  const fetchFiles = useCallback(async () => {
    // 捕获本次请求所属的工作空间，resolve 后与最新值比较，防止切换后旧响应覆盖新数据
    const ws = workspaceId;
    try {
      setFilesLoading(true);
      const list = await fetchWikiFiles(ws);
      if (latestWorkspaceIdRef.current !== ws) return; // 已切换到别的工作空间，丢弃
      setFiles(list);
      // 计算默认 slug：优先 topic，其次 log，都没有则空
      const defaultSlug = list.find(f => f.file_type === 'topic')?.slug
        ?? list.find(f => f.file_type === 'log')?.slug
        ?? '';
      // 用函数式更新读取最新 currentSlug，避免将其放入依赖数组而每次切页重拉列表
      setCurrentSlug(prev => (list.some(p => p.slug === prev) ? prev : defaultSlug));
    } catch (err) {
      if (latestWorkspaceIdRef.current !== ws) return; // 切换后的错误也不弹窗
      console.error('获取页面列表失败:', err);
      message.error('获取页面列表失败');
    } finally {
      // 仅当仍是本次请求的工作空间时才动 loading，避免把新工作空间的 loading 提前置 false
      if (latestWorkspaceIdRef.current === ws) setFilesLoading(false);
    }
  }, [workspaceId]);

  // 拉取当前页面详情
  const fetchCurrentFile = useCallback(async () => {
    // 捕获本次请求所属的工作空间 + slug，resolve 后与最新值比较，防切换竞态
    const ws = workspaceId;
    const slug = currentSlug;
    // 空 slug 不发起请求：初始态或切换工作空间清空后，slug 为空字符串，
    // 此时请求会得到 404 或意外数据，应直接跳过。
    if (!slug) {
      setFileLoading(false);
      return;
    }
    try {
      setFileLoading(true);
      const file = await fetchWikiFileContent(ws, slug);
      if (latestWorkspaceIdRef.current !== ws || latestSlugRef.current !== slug) return;
      setCurrentFile(file);
    } catch (err) {
      if (latestWorkspaceIdRef.current !== ws || latestSlugRef.current !== slug) return;
      console.error('获取页面详情失败:', err);
      setCurrentFile(null);
    } finally {
      if (latestWorkspaceIdRef.current === ws && latestSlugRef.current === slug) setFileLoading(false);
    }
  }, [workspaceId, currentSlug]);

  // 拉取配置（旧版接口，只用于设置弹窗）
  const fetchConfig = useCallback(async () => {
    const ws = workspaceId;
    try {
      const fetched = await fetchBlackboardData(ws);
      if (latestWorkspaceIdRef.current !== ws) return; // 已切换，丢弃旧响应
      setConfigData(fetched);
    } catch (err) {
      if (latestWorkspaceIdRef.current !== ws) return;
      console.error('获取黑板配置失败:', err);
    }
  }, [workspaceId, setConfigData]);

  // workspace 切换时先清空隔离数据，避免加载失败或加载窗口期暴露上一工作空间内容
  // 注意：不设 currentSlug = ''，否则 Menu 收到 selectedKeys={['']} 会崩溃
  //（Ant Design Menu.js:40 prefixCls → Cannot read properties of null）。
  // files 已清空 → Menu 不渲染（files.length === 0 显示"暂无页面"），
  // fetchFiles 异步完成后会自动设回有效 slug。
  useEffect(() => {
    setFiles([]);
    setCurrentFile(null);
    setConfigData(null);
  }, [workspaceId]);

  // 副作用：workspaceId 变化时重拉
  useEffect(() => {
    fetchFiles();
    fetchConfig();
  }, [fetchFiles, fetchConfig]);

  // URL 中的 blackboardFile 变化时，同步到 currentSlug（支持浏览器前进后退）
  useEffect(() => {
    if (blackboardFile) {
      setCurrentSlug(blackboardFile);
    }
  }, [blackboardFile]);

  // 副作用：currentSlug 变化时重拉页面详情
  // 守卫已在 fetchCurrentFile 内部处理空 slug 场景
  useEffect(() => {
    fetchCurrentFile();
  }, [fetchCurrentFile]);

  // 刷新：重新拉取列表和当前页面
  const handleRefresh = useCallback(() => {
    fetchFiles();
    fetchCurrentFile();
  }, [fetchFiles, fetchCurrentFile]);

  // 移动端选择目录后关闭 Drawer，同时同步 URL
  const handleSelectSlug = useCallback((slug: string) => {
    setCurrentSlug(slug);
    setMenuDrawerOpen(false);
    // 同步 URL：/#/blackboard?file=slug
    replaceUrl('blackboard', { file: slug });
  }, [replaceUrl]);

  // 当前选中页是否为 topic 类型：只有 topic 才允许生成 Todo 建议（log/index 不分析）
  const isCurrentTopic = files.some(f => f.slug === currentSlug && f.file_type === 'topic');

  return (
    <PageCard
      icon={<TfiBlackboard style={{ fontSize: 18 }} />}
      title="黑板"
      titleSuffix={isMobile ? <MobileDebounceIndicator workspaceId={workspaceId} /> : undefined}
      extra={
        isMobile ? (
          <MobileHeaderExtra
            onMenuClick={() => setMenuDrawerOpen(true)}
            onOpenSettings={handleOpenSettings}
            onRefresh={handleRefresh}
          />
        ) : (
          <DesktopHeaderExtra
            workspaceId={workspaceId}
            onOpenSettings={handleOpenSettings}
            onRefresh={handleRefresh}
          />
        )
      }
      contentStyle={{ padding: 0, display: 'flex', flexDirection: 'column', overflow: 'hidden' }}
    >
      <BlackboardWikiLayout
        isDark={isDark}
        isMobile={isMobile}
        files={files}
        currentFile={currentFile}
        currentSlug={currentSlug}
        onSelectSlug={handleSelectSlug}
        filesLoading={filesLoading}
        fileLoading={fileLoading}
        menuDrawerOpen={menuDrawerOpen}
        onMenuDrawerClose={() => setMenuDrawerOpen(false)}
        workspaceId={workspaceId}
        isTopic={isCurrentTopic}
        onTopicDeleted={fetchFiles}
      />

      {/* 黑板设置弹窗：Tab1 防抖设置，Tab2 提示词设置 */}
      <Modal
        title="黑板设置"
        open={settingsOpen}
        onOk={handleSaveSettings}
        onCancel={() => setSettingsOpen(false)}
        okText="保存"
        confirmLoading={settingsSaving}
        destroyOnHidden
        width={isMobile ? '90%' : 640}
      >
        <div style={{ marginBottom: 16, display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
          <span style={{ fontSize: 14, fontWeight: 500 }}>启用黑板</span>
          <Switch checked={bbEnabled} onChange={setBbEnabled} />
        </div>
        <Tabs
          activeKey={activeTab}
          onChange={(key) => setActiveTab(key as 'debounce' | 'prompt')}
          items={[
            {
              key: 'debounce',
              label: '防抖设置',
              children: (
                <DebounceSettingsTab
                  debounceSecs={debounceSecs}
                  setDebounceSecs={setDebounceSecs}
                  debounceCount={debounceCount}
                  setDebounceCount={setDebounceCount}
                  wikiTimeoutSecs={wikiTimeoutSecs}
                  setWikiTimeoutSecs={setWikiTimeoutSecs}
                />
              ),
            },
            {
              key: 'prompt',
              label: '提示词设置',
              children: (
                <PromptSettingsTab
                  wikiPrompt={wikiPrompt}
                  setWikiPrompt={setWikiPrompt}
                  onRestorePrompt={handleRestorePrompt}
                />
              ),
            },
          ]}
        />
      </Modal>
    </PageCard>
  );
}

// ─── 设置弹窗子组件（避免 Tabs children 深层嵌套）─────────────────

interface DebounceSettingsTabProps {
  debounceSecs: number | null;
  setDebounceSecs: (v: number | null) => void;
  debounceCount: number | null;
  setDebounceCount: (v: number | null) => void;
  /** Wiki 执行超时（秒） */
  wikiTimeoutSecs: number | null;
  setWikiTimeoutSecs: (v: number | null) => void;
}

/** 防抖设置 Tab：防抖周期 + 触发条数 + Wiki 执行超时，受父组件状态控制 */
function DebounceSettingsTab({ debounceSecs, setDebounceSecs, debounceCount, setDebounceCount, wikiTimeoutSecs, setWikiTimeoutSecs }: DebounceSettingsTabProps) {
  return (
    <Form layout="vertical" style={{ marginTop: 16 }}>
      <Form.Item label="防抖周期">
        <InputNumber
          value={debounceSecs}
          // 用户清空输入时 value=null，不立即回填默认值，只透传 null 给 state；
          // 保存时由 handleSaveSettings 用 ?? 兜底，避免删值瞬间被 600 覆盖
          onChange={(v) => setDebounceSecs(v)}
          min={10}
          max={3600}
          addonAfter="秒"
          style={{ width: 200 }}
        />
      </Form.Item>
      <Form.Item label="触发条数">
        <InputNumber
          value={debounceCount}
          onChange={(v) => setDebounceCount(v)}
          min={1}
          max={100}
          addonAfter="条"
          style={{ width: 200 }}
        />
      </Form.Item>
      <Form.Item
        label="Wiki 执行超时"
        // 后端会把输入值钳制到 [60, 3600]，这里同步展示边界提示；
        // 默认 300 秒（5 分钟），慢模型可调大避免被强制超时
        extra="Wiki 自动维护与 Wiki 对话的最长执行时长（后端会自动钳制到 60–3600 秒），默认 300 秒"
      >
        <InputNumber
          value={wikiTimeoutSecs}
          onChange={(v) => setWikiTimeoutSecs(v)}
          min={60}
          max={3600}
          addonAfter="秒"
          style={{ width: 200 }}
        />
      </Form.Item>
      <Form.Item extra="达到条数阈值或周期到期时，统一处理 pending 的 todo，减少频繁的 LLM 调用" />
    </Form>
  );
}

interface PromptSettingsTabProps {
  wikiPrompt: string;
  setWikiPrompt: (v: string) => void;
  onRestorePrompt: () => void;
}

/** 提示词设置 Tab：单阶段 Wiki 提示词 */
function PromptSettingsTab({
  wikiPrompt, setWikiPrompt,
  onRestorePrompt,
}: PromptSettingsTabProps) {
  return (
    <div style={{ marginTop: 16 }}>
      <div style={{ marginBottom: 20 }}>
        <Space style={{ marginBottom: 8 }}>
          <Button onClick={onRestorePrompt}>恢复默认</Button>
          <span style={{ color: '#888', fontSize: 12 }}>
            Wiki 提示词（单阶段：分析记录 + 直接编辑文件）
          </span>
        </Space>
        <Input.TextArea
          value={wikiPrompt}
          onChange={(e) => setWikiPrompt(e.target.value)}
          rows={16}
          placeholder="留空使用内置默认，如需自定义请直接在此输入"
        />
      </div>
    </div>
  );
}

/**
 * useState 黑板数据的轻封装：未来若加缓存/打点只改这里。
 * 单独抽 hook 是为了让上层组件更可测。
 */
function useStateBlackboardData() {
  return useState<BlackboardData | null>(null);
}

// ─── 桌面端 Header Extra（进度条 + 操作按钮 + 队列弹窗）─────────────────

interface DesktopHeaderExtraProps {
  workspaceId: number;
  onOpenSettings: () => void;
  onRefresh: () => void;
}

/**
 * 桌面端标题栏右侧区域：进度条 + 设置/队列/刷新按钮。
 *
 * 由 PageCard 的 extra prop 承接，取代原 BlackboardHeader 的桌面分支。
 * 「生成建议」等针对单个主题的操作已下放到内容区 TopicToolbar，Header 只保留工作空间级动作。
 */
function DesktopHeaderExtra({ workspaceId, onOpenSettings, onRefresh }: DesktopHeaderExtraProps) {
  const [queueModalVisible, setQueueModalVisible] = useState(false);
  const [queueIds, setQueueIds] = useState<number[]>([]);
  const [queueLoading, setQueueLoading] = useState(false);

  // 点击队列查看按钮：拉取黑板数据，提取 pending_record_ids
  const handleShowQueue = useCallback(async () => {
    setQueueLoading(true);
    try {
      const board = await getBlackboard(workspaceId);
      // 解析 pending_record_ids 字符串（如 "[12, 34, 56]"）为数组
      const ids: number[] = JSON.parse(board.pending_record_ids);
      setQueueIds(Array.isArray(ids) ? ids : []);
      setQueueModalVisible(true);
    } catch {
      // 静默失败：不弹 Modal，只清空列表
      setQueueIds([]);
    } finally {
      setQueueLoading(false);
    }
  }, [workspaceId]);

  return (
    <>
      {/* 防抖双进度条，占 flex 空间 */}
      <BlackboardDebounceBar workspaceId={workspaceId} />
      {/* 操作按钮组：设置/队列/刷新（主题级动作见内容区 TopicToolbar） */}
      <Space.Compact>
        <Button icon={<SettingOutlined />} onClick={onOpenSettings} title="设置" />
        <Button icon={<UnorderedListOutlined />} onClick={handleShowQueue} loading={queueLoading} title="查看队列 ID" />
        <Button type="primary" icon={<ReloadOutlined />} onClick={onRefresh}>
          刷新
        </Button>
      </Space.Compact>

      {/* 队列 ID 弹窗 */}
      <Modal
        title={
          <span>
            待处理队列 <span style={{ fontWeight: 400, fontSize: 13, color: '#888' }}>共 {queueIds.length} 条</span>
          </span>
        }
        open={queueModalVisible}
        onCancel={() => setQueueModalVisible(false)}
        footer={null}
        width={400}
      >
        {queueIds.length === 0 ? (
          <div style={{ textAlign: 'center', padding: '24px 0', color: '#999' }}>队列为空</div>
        ) : (
          <div style={{ maxHeight: 400, overflowY: 'auto' }}>
            {queueIds.map((id) => (
              <div
                key={id}
                style={{
                  padding: '6px 12px',
                  borderBottom: '1px solid #f0f0f0',
                  fontSize: 14,
                }}
              >
                {id}
              </div>
            ))}
          </div>
        )}
      </Modal>
    </>
  );
}

// ─── 移动端 Header Extra（目录/设置/刷新按钮）──────────────────────

interface MobileHeaderExtraProps {
  onMenuClick: () => void;
  onOpenSettings: () => void;
  onRefresh: () => void;
}

/**
 * 移动端标题栏右侧区域：目录/设置/刷新按钮。
 *
 * 由 PageCard 的 extra prop 承接，取代原 BlackboardHeader 的移动端分支。
 * 主题级操作（生成建议/删除）见内容区 TopicToolbar，Header 不再重复放置。
 */
function MobileHeaderExtra({ onMenuClick, onOpenSettings, onRefresh }: MobileHeaderExtraProps) {
  return (
    <Space.Compact size="small">
      <Button icon={<MenuOutlined />} onClick={onMenuClick} title="目录" />
      <Button icon={<SettingOutlined />} onClick={onOpenSettings} title="设置" />
      <Button type="primary" icon={<ReloadOutlined />} onClick={onRefresh} />
    </Space.Compact>
  );
}

// ─── 移动端防抖文字指示器 ─────────────────────────────────────

interface MobileDebounceIndicatorProps {
  workspaceId: number;
}

/**
 * 移动端防抖状态文字指示器。
 *
 * 监听 blackboardDebounceStatus WebSocket 事件，
 * 在极小空间内用文字显示当前防抖状态，替代桌面端的双进度条。
 * - 刷新中：绿色 "刷新中"
 * - 已触发阈值：绿色 "{pending} 条待刷"
 * - 等待中：灰色 "倒计时 {remaining}s"
 * - 无状态：不渲染
 */
function MobileDebounceIndicator({ workspaceId }: MobileDebounceIndicatorProps) {
  const [status, setStatus] = useState<BlackboardDebounceStatus | null>(null);

  useEffect(() => {
    const handler = (e: Event) => {
      const s = (e as CustomEvent<BlackboardDebounceStatus>).detail;
      if (s.workspace_id !== workspaceId) return;
      setStatus(s);
    };
    window.addEventListener('blackboardDebounceStatus', handler);
    return () => window.removeEventListener('blackboardDebounceStatus', handler);
  }, [workspaceId]);

  if (!status) return null;

  const { pending_count, threshold, remaining_secs, refreshing } = status;

  // 刷新中
  if (refreshing) {
    return <span style={{ fontSize: 11, color: '#52c41a' }}>刷新中</span>;
  }
  // 已触发阈值
  if (pending_count >= threshold) {
    return <span style={{ fontSize: 11, color: '#52c41a' }}>{pending_count} 条待刷</span>;
  }
  // 有待处理但未达阈值
  if (pending_count > 0) {
    return <span style={{ fontSize: 11, color: '#888' }}>{pending_count}/{threshold} 条</span>;
  }
  // 等待中，有倒计时
  if (remaining_secs >= 0) {
    return <span style={{ fontSize: 11, color: '#888' }}>倒计时 {remaining_secs}s</span>;
  }
  return null;
}

// ─── 黑板倒计时进度条 ───────────────────────────────────────────

interface BlackboardDebounceBarProps {
  /** 当前工作空间 ID，用于过滤事件 */
  workspaceId: number;
}

/**
 * 黑板防抖倒计时双进度条组件。
 *
 * 监听 blackboardDebounceStatus WebSocket 事件，渲染：
 * - 时间进度条（蓝色/绿色）
 * - 条数进度条（蓝色/绿色）
 * - 点击整体弹出详情，同时显示时间和条数数据
 */
function BlackboardDebounceBar({ workspaceId }: BlackboardDebounceBarProps) {
  const [status, setStatus] = useState<BlackboardDebounceStatus | null>(null);
  const [showDetail, setShowDetail] = useState(false);

  useEffect(() => {
    const handler = (e: Event) => {
      const s = (e as CustomEvent<BlackboardDebounceStatus>).detail;
      if (s.workspace_id !== workspaceId) return;
      setStatus(s);
    };
    window.addEventListener('blackboardDebounceStatus', handler);
    return () => window.removeEventListener('blackboardDebounceStatus', handler);
  }, [workspaceId]);

  if (!status) return null;

  const { pending_count, threshold, debounce_secs, remaining_secs, refreshing } = status;
  const isThresholdMet = pending_count >= threshold;
  const hasTimer = remaining_secs >= 0;

  // 时间进度（整数，已过时间正向累加，0% → 100%）
  const elapsed = hasTimer ? debounce_secs - remaining_secs : 0;
  const timePercent = hasTimer
    ? Math.floor(Math.min(100, (elapsed / debounce_secs) * 100))
    : 0;
  const timeColor = isThresholdMet || refreshing ? '#52c41a' : '#2080f0';

  // 条数进度（整数）
  const countPercent = threshold > 0
    ? Math.floor(Math.min(100, (pending_count / threshold) * 100))
    : 0;
  const countColor = isThresholdMet || refreshing ? '#52c41a' : '#2080f0';

  // hasTimer=false 时区分：pending>0 表示数据已入队等待刷新，否则才是真正的等待中
  const timeLabel = hasTimer
    ? `${elapsed}s / ${debounce_secs}s`
    : pending_count > 0
      ? '等待刷新'
      : '等待中';
  const countLabel = `${pending_count} / ${threshold} 条`;

  return (
    <div style={{ position: 'relative', flex: 1, marginRight: 16 }}>
      {/* 整个组件可点击，弹出详情 */}
      <div
        style={{ cursor: 'pointer' }}
        onClick={() => setShowDetail(v => !v)}
      >
        <div style={{ marginBottom: 6 }}>
          <Progress
            percent={timePercent}
            size="small"
            strokeColor={timeColor}
            trailColor="rgba(0,0,0,0.06)"
            status={refreshing ? 'active' : 'normal'}
          />
        </div>
        <div>
          <Progress
            percent={countPercent}
            size="small"
            strokeColor={countColor}
            trailColor="rgba(0,0,0,0.06)"
            status={refreshing ? 'active' : 'normal'}
          />
        </div>
      </div>

      {/* 点击弹出的详情气泡 */}
      {showDetail && (
        <div
          style={{
            position: 'absolute',
            top: '100%',
            right: 0,
            marginTop: 4,
            background: '#fff',
            border: '1px solid #ddd',
            borderRadius: 6,
            padding: '8px 12px',
            fontSize: 12,
            color: '#444',
            boxShadow: '0 2px 8px rgba(0,0,0,0.15)',
            zIndex: 100,
            whiteSpace: 'nowrap',
          }}
          onClick={() => setShowDetail(false)}
        >
          <div style={{ marginBottom: 4 }}>
            ⏱ 时间: <b>{timeLabel}</b>
          </div>
          <div style={{ marginBottom: refreshing ? 4 : 0 }}>
            📊 条数: <b>{countLabel}</b>
          </div>
          {refreshing && (
            <div style={{ color: '#52c41a' }}>刷新中...</div>
          )}
        </div>
      )}
    </div>
  );
}

interface BlackboardWikiLayoutProps {
  isDark: boolean;
  isMobile: boolean;
  files: WikiFileItem[];
  currentFile: WikiFileContent | null;
  currentSlug: string;
  onSelectSlug: (slug: string) => void;
  filesLoading: boolean;
  fileLoading: boolean;
  /** 移动端目录 Drawer 是否打开 */
  menuDrawerOpen: boolean;
  /** 移动端关闭目录 Drawer 的回调 */
  onMenuDrawerClose: () => void;
  /** 当前工作空间 ID，用于 Wiki 相对路径链接 */
  workspaceId: number;
  /** 当前页是否为 topic 类型（仅 topic 渲染主题工具条） */
  isTopic: boolean;
  /** 删除当前主题后的回调：重拉文件列表，列表更新会自动把 currentSlug 切到剩余 topic */
  onTopicDeleted: () => void;
}

/**
 * Wiki 布局：桌面端左侧固定目录树 + 右侧内容区，移动端侧边栏收入 Drawer。
 *
 * 桌面端（≥768px）：
 *   ┌──────────┬────────────────────────────────┐
 *   │ 目录树   │         Markdown 内容区         │
 *   │  220px   │          flex: 1               │
 *   └──────────┴────────────────────────────────┘
 *
 * 移动端（<768px）：
 *   - 固定侧边栏隐藏，内容区全宽
 *   - Header 中有"目录"按钮，点击打开 Drawer
 *   - 选择目录项后自动关闭 Drawer
 */
function BlackboardWikiLayout(props: BlackboardWikiLayoutProps) {
  const {
    isDark, isMobile, files, currentFile, currentSlug,
    onSelectSlug, filesLoading, fileLoading,
    menuDrawerOpen, onMenuDrawerClose, workspaceId,
    isTopic, onTopicDeleted,
  } = props;

  // 构造 Menu items（useMemo 防止每次父组件重渲染都重建新数组→触发 Menu 内部 prefixCls null 崩溃）
  const menuItems = useMemo(() => [
    // 主题页分组
    {
      key: 'topics-group',
      label: <span style={{ fontWeight: 600, fontSize: 12, color: isDark ? '#aaa' : '#666' }}>主题页面</span>,
      type: 'group' as const,
      children: files.filter(f => f.file_type === 'topic').map(f => ({
        key: f.slug,
        label: f.slug,
        type: 'item' as const,
      })),
    },
    // log 页
    ...files.filter(f => f.file_type === 'log').map(f => ({
      key: f.slug,
      label: '执行日志',
      type: 'item' as const,
    })),
  ], [files, isDark]);

  const sidebarBg = isDark ? '#1a1a1a' : '#fafafa';
  const sidebarBorder = isDark ? '#333' : '#f0f0f0';

  // 渲染目录内容（useMemo 防止 filesLoading/fileLoading/currentFile 等变化时 Menu 被不必要地重渲染）
  const sidebarContent = useMemo(() => (
    filesLoading ? (
      <Skeleton active paragraph={{ rows: 6 }} style={{ padding: '0 12px' }} />
    ) : files.length === 0 ? (
      <div style={{ padding: '24px 12px', textAlign: 'center', color: isDark ? '#666' : '#999', fontSize: 12 }}>
        暂无页面
      </div>
    ) : (
      <Menu
        mode="inline"
        selectedKeys={[currentSlug]}
        onClick={({ key }) => onSelectSlug(key as string)}
        style={{ background: 'transparent', borderRight: 'none' }}
        theme={isDark ? 'dark' : 'light'}
        items={menuItems}
      />
    )
  ), [filesLoading, files, currentSlug, isDark, onSelectSlug, menuItems]);

  // 移动端：内容区全宽，目录通过 Drawer 呈现
  if (isMobile) {
    return (
      <div style={{ flex: 1, display: 'flex', flexDirection: 'column', overflow: 'hidden', minHeight: 0 }}>
        {/* 内容区 */}
        <div style={{ flex: 1, overflow: 'auto', padding: '12px', minWidth: 0 }}>
          {/* 主题页操作工具条：生成建议 + 删除当前主题（仅 topic 渲染） */}
          {isTopic && (
            <TopicToolbar
              workspaceId={workspaceId}
              slug={currentSlug}
              onDeleted={onTopicDeleted}
              isMobile={isMobile}
              isDark={isDark}
            />
          )}
          {fileLoading ? (
            <Skeleton active paragraph={{ rows: 10 }} />
          ) : !currentFile || !currentFile.content || currentFile.content.trim().length === 0 ? (
            <BlackboardEmpty isDark={isDark} />
          ) : (
            <BlackboardContent isDark={isDark} content={currentFile.content} workspaceId={workspaceId} />
          )}
        </div>

        {/* 移动端目录 Drawer */}
        <Drawer
          title="目录"
          placement="left"
          width={280}
          onClose={onMenuDrawerClose}
          open={menuDrawerOpen}
          styles={{ body: { padding: 0, background: sidebarBg } }}
        >
          {sidebarContent}
        </Drawer>
      </div>
    );
  }

  // 桌面端：固定侧边栏 + 内容区
  return (
    <div style={{ flex: 1, display: 'flex', overflow: 'hidden', minHeight: 0 }}>
      {/* 左侧目录树 */}
      <div
        style={{
          width: 220,
          flexShrink: 0,
          background: sidebarBg,
          borderRight: `1px solid ${sidebarBorder}`,
          overflowY: 'auto',
          padding: '8px 0',
        }}
      >
        {sidebarContent}
      </div>

      {/* 右侧内容区 */}
      <div style={{ flex: 1, overflow: 'auto', padding: '16px 24px', minWidth: 0 }}>
        {/* 主题页操作工具条：生成建议 + 删除当前主题（仅 topic 渲染） */}
        {isTopic && (
          <TopicToolbar
            workspaceId={workspaceId}
            slug={currentSlug}
            onDeleted={onTopicDeleted}
            isMobile={isMobile}
            isDark={isDark}
          />
        )}
        {fileLoading ? (
          <Skeleton active paragraph={{ rows: 10 }} />
        ) : !currentFile || !currentFile.content || currentFile.content.trim().length === 0 ? (
          <BlackboardEmpty isDark={isDark} />
        ) : (
          <BlackboardContent isDark={isDark} content={currentFile.content} workspaceId={workspaceId} />
        )}
      </div>
    </div>
  );
}

interface TopicToolbarProps {
  /** 当前工作空间 ID，传给 ProposalButton 拼接 topic 文件路径 */
  workspaceId: number;
  /** 当前主题 slug，删除与生成建议都作用于它 */
  slug: string;
  /** 删除成功后回调：父组件重拉文件列表，列表更新会自动把 currentSlug 切到剩余 topic */
  onDeleted: () => void;
  /** 移动端用 small 尺寸与其他工具按钮对齐 */
  isMobile: boolean;
  /** 暗色主题下调整工具条背景与边框色 */
  isDark: boolean;
}

/**
 * 主题页操作工具条：生成建议 + 删除当前主题。
 *
 * 取代原先放在黑板 Header 里的「生成建议」按钮——Header 是工作空间级入口，
 * 而生成建议/删除都针对「当前正在看的主题」，下放到内容区更符合作用域，
 * 也让每个主题页自带操作入口。删除走二次确认防误删；删除后由父组件重拉列表。
 */
function TopicToolbar({ workspaceId, slug, onDeleted, isMobile, isDark }: TopicToolbarProps) {
  const [deleteModalOpen, setDeleteModalOpen] = useState(false);
  const [deleting, setDeleting] = useState(false);

  const handleDelete = async () => {
    setDeleting(true);
    try {
      await performDelete(workspaceId, slug, onDeleted);
      setDeleteModalOpen(false);
    } finally {
      setDeleting(false);
    }
  };

  return (
    <>
      <div
        style={{
          // sticky：内容区滚动时工具条常驻顶部，操作始终可达
          position: 'sticky',
          top: 0,
          zIndex: 5,
          display: 'flex',
          justifyContent: 'flex-end',
          gap: 8,
          // 与下方内容留间隔；背景与边框做成一条可视条，避免 sticky 时与正文粘连
          marginBottom: 12,
          padding: '6px 0',
          background: isDark ? '#1a1a1a' : '#fafafa',
          borderBottom: `1px solid ${isDark ? '#333' : '#f0f0f0'}`,
        }}
      >
        <ProposalButton
          workspaceId={workspaceId}
          slug={slug}
          buttonSize={isMobile ? 'small' : 'middle'}
          showLabel={!isMobile}
        />
        <Button
          danger
          icon={<DeleteOutlined />}
          onClick={() => setDeleteModalOpen(true)}
          size={isMobile ? 'small' : 'middle'}
        >
          {isMobile ? '' : '删除主题'}
        </Button>
      </div>
      <Modal
        title="删除主题"
        open={deleteModalOpen}
        onOk={handleDelete}
        onCancel={() => setDeleteModalOpen(false)}
        okText="删除"
        okType="danger"
        cancelText="取消"
        confirmLoading={deleting}
      >
        <p style={{ margin: 0 }}>确定删除主题「{slug}」？该 Wiki 文件将被永久删除，已创建的 Todo 不受影响。</p>
      </Modal>
    </>
  );
}

/** 执行删除请求：成功提示并触发列表重拉；失败 toast 透出原因，不关闭确认框外层流程。 */
async function performDelete(workspaceId: number, slug: string, onDeleted: () => void): Promise<void> {
  try {
    await deleteWikiFile(workspaceId, slug);
    message.success(`已删除主题「${slug}」`);
    onDeleted();
  } catch (err) {
    message.error('删除失败: ' + (err instanceof Error ? err.message : String(err)));
  }
}

interface BlackboardContentProps {
  isDark: boolean;
  content: string;
  /** 当前工作空间 ID，用于 Wiki 相对路径链接导航 */
  workspaceId?: number;
}

/** 真正渲染 Markdown：XMarkdown 内部走 DOMPurify 防止 XSS */
function BlackboardContent(props: BlackboardContentProps) {
  const isDark = props.isDark;
  const { selectWiki } = useViewState();
  // 前端兼容兜底：渲染前再剥一次外层 fenced markdown，保证历史脏数据也能正常显示
  const renderedContent = normalizeBlackboardMarkdown(props.content);
  return (
    <div
      style={{
        // 主题适配：暗色用近黑容器，亮色用白底
        background: isDark ? '#1f1f1f' : '#fff',
        borderRadius: 8,
        padding: 16,
        minHeight: 200,
        lineHeight: 1.8,
        fontSize: 14,
        color: isDark ? '#e0e0e0' : '#333',
      }}
    >
      <XMarkdown
        // 强制纯文本：XMarkdown 默认会注入 inline style，
        // className 包一层让主题色与外层容器保持一致
        className={isDark ? 'x-markdown-dark' : 'x-markdown-light'}
        content={renderedContent}
        // 覆盖 a 标签渲染：让 ntd://todo/{id} 走内部导航，./slug 走 Wiki 导航
        components={{ a: (linkProps) => (
          <TodoLink
            {...linkProps}
            workspaceId={props.workspaceId}
            selectWiki={selectWiki}
          />
        )}}
        // DOMPurify 默认会拒绝 ntd:// 等未知协议，会把整条链接剥成纯文本。
        // 显式允许 ntd 协议 + 以 / 开头的内部相对路径（如 /#/items?id=16&panel=post&record=6513），
        // 其它未知协议仍被拒绝。
        dompurifyConfig={{
          ALLOWED_URI_REGEXP: /^(?:(?:https?|mailto|tel|ntd):|\/|\.\/)/i,
        }}
      />
    </div>
  );
}

interface BlackboardEmptyProps {
  isDark: boolean;
}

/** 空状态：占位文案，明确告诉用户"任务执行后会自动出现内容" */
function BlackboardEmpty(props: BlackboardEmptyProps) {
  return (
    <div
      style={{
        textAlign: 'center',
        padding: '48px 0',
        color: props.isDark ? '#666' : '#999',
      }}
    >
      <p style={{ fontSize: 16, marginBottom: 8, color: props.isDark ? '#aaa' : '#666' }}>
        暂无内容
      </p>
      <p>任务执行后将自动更新黑板内容</p>
    </div>
  );
}
