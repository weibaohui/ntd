/**
 * WikiViewPage — 独立渲染单个 wiki 文件的页面。
 *
 * 通过 URL hash 路由定位：/#/wiki?workspace=1&slug=auth-module
 * 用于在黑板 Wiki 中点击链接直接打开对应文件。
 */

import { useState, useEffect, useCallback } from 'react';
import { Skeleton, message } from 'antd';
import { XMarkdown } from '@ant-design/x-markdown';
import { useTheme } from '@/hooks/useTheme';
import { PageCard } from '@/components/common/PageCard';
import { useViewState } from '@/hooks/useViewState';
import { TfiBlackboard } from 'react-icons/tfi';

/** Wiki 文件内容 */
interface WikiFileContent {
  slug: string;
  content: string;
}

/** 拉取单个 Wiki 文件内容（原生 fetch，手动写 v1 路径） */
async function fetchWikiFileContent(workspaceId: number, slug: string): Promise<WikiFileContent> {
  const res = await fetch(`/api/v1/workspaces/${workspaceId}/wiki/files/${encodeURIComponent(slug)}`);
  if (!res.ok) {
    throw new Error(`HTTP ${res.status}`);
  }
  const json = (await res.json()) as { data?: WikiFileContent };
  if (!json.data) {
    throw new Error('Empty response body');
  }
  return json.data;
}

export function WikiViewPage() {
  const { themeMode } = useTheme();
  const isDark = themeMode === 'dark';
  const { wikiSlug, showView } = useViewState();

  // 从 URL hash 解析 workspace 和 slug
  const [workspaceId, setWorkspaceId] = useState<number | null>(null);
  const [slug, setSlug] = useState<string | null>(null);
  const [content, setContent] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  // 解析 URL 参数
  useEffect(() => {
    const hash = window.location.hash || '';
    const hashWithoutHash = hash.startsWith('#') ? hash.slice(1) : hash;
    const [, search] = hashWithoutHash.split('?', 2);
    const params = new URLSearchParams(search || '');
    const ws = params.get('workspace');
    const sl = params.get('slug');
    setWorkspaceId(ws ? Number(ws) : null);
    setSlug(sl);
  }, [wikiSlug]);

  // 拉取文件内容
  const loadContent = useCallback(async () => {
    if (!workspaceId || !slug) return;
    setLoading(true);
    setContent(null);
    try {
      const file = await fetchWikiFileContent(workspaceId, slug);
      setContent(file.content);
    } catch (err) {
      console.error('获取 Wiki 文件失败:', err);
      message.error('获取文件失败');
    } finally {
      setLoading(false);
    }
  }, [workspaceId, slug]);

  useEffect(() => {
    loadContent();
  }, [loadContent]);

  const handleBack = () => {
    // 回到黑板主视图（blackboard 视图）
    showView('blackboard');
  };

  return (
    <PageCard
      icon={<TfiBlackboard style={{ fontSize: 18 }} />}
      title={slug || 'Wiki'}
      extra={
        <button
          onClick={handleBack}
          style={{
            background: 'none',
            border: 'none',
            cursor: 'pointer',
            color: 'var(--color-primary, #1677ff)',
            fontSize: 13,
            padding: '4px 8px',
          }}
        >
          ← 回到黑板
        </button>
      }
      contentStyle={{ padding: 0, overflow: 'auto' }}
    >
      <div
        style={{
          padding: '16px 24px',
          minHeight: 200,
        }}
      >
        {loading ? (
          <Skeleton active paragraph={{ rows: 10 }} />
        ) : !content ? (
          <div style={{ textAlign: 'center', padding: 48, color: isDark ? '#666' : '#999' }}>
            文件不存在或加载失败
          </div>
        ) : (
          <div
            style={{
              background: isDark ? '#1f1f1f' : '#fff',
              borderRadius: 8,
              padding: 16,
              lineHeight: 1.8,
              fontSize: 14,
              color: isDark ? '#e0e0e0' : '#333',
            }}
          >
            <XMarkdown
              content={content}
            />
          </div>
        )}
      </div>
    </PageCard>
  );
}
