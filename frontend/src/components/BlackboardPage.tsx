/**
 * BlackboardPage — 黑板页面。
 *
 * 渲染工作空间的黑板内容（Markdown 格式），
 * 支持手动刷新和 ntd://todo/{id} 链接跳转。
 */

import { useState, useEffect, useCallback } from 'react';
import { Button, Typography, Skeleton, message } from 'antd';
import { ReloadOutlined } from '@ant-design/icons';

const { Title } = Typography;

// 黑板数据接口（匹配后端 API 返回结构）
interface BlackboardData {
  id: number;
  workspace_id: number;
  content: string;
  updated_at: string | null;
}

/**
 * 自定义 Markdown 链接渲染器：识别 ntd://todo/{id} 协议。
 *
 * 当用户在黑板中点击类似 (来源: [todo_123](ntd://todo/123)) 的链接时，
 * 解析 todo_id 并通过 URL 参数跳转到事项详情页。
 */
function renderMarkdownLinks(content: string): string {
  // 将 ntd://todo/{id} 替换为普通的 #/items?id={id} HTML 链接
  // 使用正则匹配，不依赖额外依赖
  return content.replace(
    /ntd:\/\/todo\/(\d+)/g,
    (_match, todoId: string) => `#/items?id=${todoId}`
  );
}

/**
 * 黑板页面组件。
 *
 * 布局：
 *   ┌──────────────────────────────────┐
 *   │ 黑板                     [刷新按钮] |
 *   ├──────────────────────────────────┤
 *   │            Markdown 内容          │
 *   │  (或空状态提示"暂无内容...")        │
 *   └──────────────────────────────────┘
 */
export function BlackboardPage() {
  const [data, setData] = useState<BlackboardData | null>(null);
  const [loading, setLoading] = useState(true);
  const [refreshing, setRefreshing] = useState(false);

  // 当前工作空间 ID — 从 URL 参数或全局状态获取
  // 后端 API 路径为 /api/workspaces/{workspace_id}/blackboard
  // 这里从 URL 的 workspace 参数获取，默认使用 1
  const [workspaceId] = useState<number>(() => {
    const ws = new URLSearchParams(window.location.search).get('workspace');
    return ws ? Number(ws) : 1;
  });

  // 获取黑板内容
  const fetchBlackboard = useCallback(async () => {
    try {
      setLoading(true);
      const res = await fetch(`/api/workspaces/${workspaceId}/blackboard`);
      if (!res.ok) {
        throw new Error(`HTTP ${res.status}`);
      }
      const json = await res.json();
      if (json.data) {
        setData(json.data as BlackboardData);
      }
    } catch (err) {
      console.error('获取黑板失败:', err);
      message.error('获取黑板内容失败');
    } finally {
      setLoading(false);
    }
  }, [workspaceId]);

  // 手动刷新黑板
  const handleRefresh = useCallback(async () => {
    try {
      setRefreshing(true);
      const res = await fetch(`/api/workspaces/${workspaceId}/blackboard/refresh`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
      });
      if (!res.ok) {
        throw new Error(`HTTP ${res.status}`);
      }
      message.success('黑板刷新已触发，请稍后查看');
      // 延迟 2 秒后自动拉取最新内容
      setTimeout(() => {
        fetchBlackboard();
      }, 2000);
    } catch (err) {
      console.error('刷新黑板失败:', err);
      message.error('刷新黑板失败');
    } finally {
      setRefreshing(false);
    }
  }, [workspaceId, fetchBlackboard]);

  // 页面加载时自动获取黑板内容
  useEffect(() => {
    fetchBlackboard();
  }, [fetchBlackboard]);

  // 渲染 Markdown 内容为 HTML（简单换行 + 链接处理）
  const renderContent = (text: string): string => {
    // 先处理特殊链接协议
    let html = renderMarkdownLinks(text);
    // 简单的 Markdown 到 HTML 转换（标题、列表、代码块）
    // 使用 @ant-design/x-markdown 需要额外安装，这里先用基础渲染
    // 将 \n 转换为 <br/>
    html = html.replace(/\n/g, '<br/>');
    return html;
  };

  return (
    <div style={{ padding: '16px 24px', height: '100%', overflow: 'auto' }}>
      {/* 顶部标题栏：左侧标题 + 右侧刷新按钮 */}
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'space-between',
          marginBottom: 16,
        }}
      >
        <Title level={4} style={{ margin: 0 }}>
          黑板
        </Title>
        <Button
          type="primary"
          icon={<ReloadOutlined />}
          loading={refreshing}
          onClick={handleRefresh}
          disabled={loading || data === null}
        >
          {refreshing ? '更新中...' : '刷新'}
        </Button>
      </div>

      {/* 内容区域 */}
      {loading ? (
        <Skeleton active paragraph={{ rows: 8 }} />
      ) : data && data.content ? (
        <div
          style={{
            background: '#fff',
            borderRadius: 8,
            padding: 16,
            minHeight: 200,
            lineHeight: 1.8,
            fontSize: 14,
          }}
          dangerouslySetInnerHTML={{ __html: renderContent(data.content) }}
        />
      ) : (
        <div
          style={{
            textAlign: 'center',
            padding: '48px 0',
            color: '#999',
          }}
        >
          <p style={{ fontSize: 16, marginBottom: 8 }}>暂无内容</p>
          <p>任务执行后将自动更新黑板内容</p>
        </div>
      )}
    </div>
  );
}
