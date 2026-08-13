/**
 * useBlackboardWiki — 黑板 Wiki 数据族 hook（096-W4-4 产物）。
 *
 * 承接原 BlackboardPage 主组件的整块数据逻辑：
 * - 文件列表 / 当前文件内容 / 黑板配置（设置弹窗数据源）的加载与缓存；
 * - 工作空间切换时的数据清空与重拉；
 * - URL `blackboardFile` 参数与当前 slug 的双向同步入口；
 * - latest-wins 竞态防护（切换工作空间/文件后晚到的响应直接丢弃）。
 *
 * 函数体逐字搬自主组件原实现，行为等价。
 */

import { useCallback, useEffect, useRef, useState } from 'react';
import { message } from 'antd';
import { useViewState } from '@/hooks/useViewState';
import { fetchBlackboardData, fetchWikiFileContent, fetchWikiFiles } from '@/components/blackboard/api';
import type { BlackboardData, WikiFileContent, WikiFileItem } from '@/components/blackboard/types';

export interface BlackboardWikiState {
  files: WikiFileItem[];
  currentFile: WikiFileContent | null;
  currentSlug: string;
  filesLoading: boolean;
  fileLoading: boolean;
  /** 黑板配置（设置弹窗的数据源；保存成功后由弹窗经 setConfigData 回写） */
  configData: BlackboardData | null;
  setConfigData: (d: BlackboardData | null) => void;
  /** 切换当前页（slug）；URL 同步由调用方负责（保持与原实现分工一致） */
  setCurrentSlug: React.Dispatch<React.SetStateAction<string>>;
  /** 重拉文件列表（主题删除后调用——列表更新会自动切到剩余 topic） */
  fetchFiles: () => Promise<void>;
  /** 刷新：重拉列表 + 当前页面内容 */
  refresh: () => void;
}

export function useBlackboardWiki(workspaceId: number): BlackboardWikiState {
  // Wiki 化数据状态
  const [files, setFiles] = useState<WikiFileItem[]>([]);
  const [currentFile, setCurrentFile] = useState<WikiFileContent | null>(null);
  const [currentSlug, setCurrentSlug] = useState<string>('');
  const [filesLoading, setFilesLoading] = useState(true);
  const [fileLoading, setFileLoading] = useState(false);
  // 旧版数据（配置用）
  const [configData, setConfigData] = useState<BlackboardData | null>(null);

  // URL 中的 blackboardFile（浏览器前进后退同步源）
  const { blackboardFile } = useViewState();

  // 防切换竞态：ref 始终持有「最新一次渲染」的 workspaceId / currentSlug。
  // fetch 回调在 await 前捕获闭包里的旧 key，resolve 后与 ref 比较——
  // 不一致说明期间已切换工作空间/文件，晚到的响应直接丢弃，避免覆盖新工作空间的数据。
  // 与 useLoopExecutions/useExecutionHistory 的 cancelledRef 思路一致（latest-wins）。
  const latestWorkspaceIdRef = useRef(workspaceId);
  latestWorkspaceIdRef.current = workspaceId;
  const latestSlugRef = useRef(currentSlug);
  latestSlugRef.current = currentSlug;

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
  }, [workspaceId]);

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
  const refresh = useCallback(() => {
    fetchFiles();
    fetchCurrentFile();
  }, [fetchFiles, fetchCurrentFile]);

  return {
    files,
    currentFile,
    currentSlug,
    filesLoading,
    fileLoading,
    configData,
    setConfigData,
    setCurrentSlug,
    fetchFiles,
    refresh,
  };
}
