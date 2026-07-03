/**
 * BlackboardPage — 黑板页面。
 *
 * 渲染工作空间的黑板内容（Markdown 格式），
 * 支持手动刷新和 ntd://todo/{id} 内部链接跳转。
 *
 * 布局：
 *   ┌──────────────────────────────────┐
 *   │ 黑板                     [刷新按钮] |
 *   ├──────────────────────────────────┤
 *   │            Markdown 内容          │
 *   │  (或空状态提示"暂无内容...")        │
 *   └──────────────────────────────────┘
 */

import { useState, useEffect, useCallback, useMemo } from 'react';
import { Button, Typography, Skeleton, message, Modal, Form, InputNumber, Space } from 'antd';
import { ReloadOutlined, SettingOutlined } from '@ant-design/icons';
import { XMarkdown } from '@ant-design/x-markdown';
import { useTheme } from '@/hooks/useTheme';
import { useViewState } from '@/hooks/useViewState';
import * as db from '@/utils/database';

const { Title } = Typography;

/** 黑板 API 返回的 JSON 形状（与后端 BlackboardResponse 对应） */
interface BlackboardData {
  id: number;
  workspace_id: number;
  content: string;
  updated_at: string | null;
}

/** ntd://todo/{id} 协议的前缀，用于解析 LLM 注入的内部链接 */
const NTD_TODO_PROTOCOL_PREFIX = 'ntd://todo/';

/** URL search 参数 `workspace` 的键名 */
const URL_WORKSPACE_PARAM = 'workspace';

/** 默认工作空间 ID（首屏兜底，避免 URL 未带参时无 workspace） */
const DEFAULT_WORKSPACE_ID = 1;

/** 触发刷新后延迟拉取最新内容的时间，让 LLM 任务有时间写库 */
const REFRESH_POLL_DELAY_MS = 2000;

/** Markdown 链接组件 props 形状（XMarkdown ComponentProps 简化版） */
interface MarkdownLinkProps extends React.AnchorHTMLAttributes<HTMLAnchorElement> {
  href?: string;
  children?: React.ReactNode;
}

/**
 * Markdown 链接渲染器：识别 ntd://todo/{id} 内部协议。
 *
 * 行为：
 * - href 以 ntd://todo/ 开头 → 渲染为可点击的"内链"按钮，
 *   点击时通过 useViewState.selectTodo 导航到事项详情，
 *   阻止浏览器尝试解析 ntd:// 自定义协议导致"找不到应用"提示。
 * - 其他 href（http/https/mailto 等）→ 用原生 <a target="_blank"> 打开外部链接。
 */
function TodoLink(props: MarkdownLinkProps): React.ReactElement {
  // 用 hook 不能放在条件分支里：TodoLink 总是组件实例，调用安全
  const { selectTodo } = useViewState();
  const href = props.href ?? '';
  // 解析 ntd://todo/{id} → 提取纯数字 id
  const isInternal = href.startsWith(NTD_TODO_PROTOCOL_PREFIX);
  const todoId = isInternal ? Number(href.slice(NTD_TODO_PROTOCOL_PREFIX.length)) : NaN;

  // 内部链接：用 button 风格 + onClick，避免浏览器把 ntd:// 解释成未知协议
  if (isInternal && Number.isFinite(todoId)) {
    return (
      <a
        {...props}
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
        {props.children}
      </a>
    );
  }

  // 外部链接：新窗口打开 + rel=noopener 防 tabnabbing
  return (
    <a {...props} target="_blank" rel="noopener noreferrer">
      {props.children}
    </a>
  );
}

/** 从 URL ?workspace=N 解析工作空间 ID；解析失败时返回默认值 */
function resolveWorkspaceFromUrl(): number {
  // 在浏览器外（如 SSR/测试）调用 window 会炸；外层先保证只在浏览器跑
  const raw = new URLSearchParams(window.location.search).get(URL_WORKSPACE_PARAM);
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

/** 拉取黑板内容的纯函数，便于测试与复用 */
async function fetchBlackboardData(workspaceId: number): Promise<BlackboardData> {
  // 走原生 fetch：项目其它页也直接用 fetch，引入 axios 不会带来收益
  const res = await fetch(`/api/workspaces/${workspaceId}/blackboard`);
  if (!res.ok) {
    throw new Error(`HTTP ${res.status}`);
  }
  // 后端返回 { data: {...} }：解包 data 字段
  const json = (await res.json()) as { data?: BlackboardData };
  if (!json.data) {
    throw new Error('Empty response body');
  }
  return json.data;
}

/** 触发手动刷新，返回是否成功 */
async function triggerRefresh(workspaceId: number): Promise<void> {
  const res = await fetch(`/api/workspaces/${workspaceId}/blackboard/refresh`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
  });
  if (!res.ok) {
    throw new Error(`HTTP ${res.status}`);
  }
}

export function BlackboardPage({ workspaceId: propWorkspaceId }: { workspaceId?: number | null }) {
  // 主题：决定黑板容器背景与文字色
  const { themeMode } = useTheme();
  const isDark = themeMode === 'dark';
  // 派生值（不再 useState）：切换工作空间时自动跟随 prop 变化
  const workspaceId = useEffectiveWorkspaceId(propWorkspaceId);

  // 数据状态：data 既是"内容"也是"是否加载完成"的判断
  const [data, setData] = useStateBlackboardData();
  const [refreshing, setRefreshing] = useState(false);
  // 设置弹窗状态
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [settingsSaving, setSettingsSaving] = useState(false);
  const [debounceSecs, setDebounceSecs] = useState<number>(600);
  const [debounceCount, setDebounceCount] = useState<number>(10);

  // 打开设置弹窗：先拉取最新 config
  const handleOpenSettings = useCallback(async () => {
    setSettingsOpen(true);
    try {
      const cfg = await db.getConfig();
      setDebounceSecs(cfg.blackboard_debounce_secs ?? 600);
      setDebounceCount(cfg.blackboard_debounce_count ?? 10);
    } catch {
      setDebounceSecs(600);
      setDebounceCount(10);
    }
  }, []);

  // 保存设置
  const handleSaveSettings = useCallback(async () => {
    setSettingsSaving(true);
    try {
      const current = await db.getConfig();
      await db.updateConfig({ ...current, blackboard_debounce_secs: debounceSecs, blackboard_debounce_count: debounceCount });
      message.success('设置已保存');
      setSettingsOpen(false);
    } catch (err) {
      message.error('保存失败: ' + (err instanceof Error ? err.message : String(err)));
    } finally {
      setSettingsSaving(false);
    }
  }, [debounceSecs, debounceCount]);

  // 拉取（受 workspaceId 变化驱动）：useCallback 稳定引用，让 useEffect 只在 id 变时重跑
  const fetchData = useCallback(async () => {
    try {
      const fetched = await fetchBlackboardData(workspaceId);
      setData(fetched);
    } catch (err) {
      // 业务错误不影响页面骨架：仅弹 toast 让用户知晓
      console.error('获取黑板失败:', err);
      message.error('获取黑板内容失败');
    }
  }, [workspaceId, setData]);

  // 副作用：workspaceId 变化时重拉
  useEffect(() => {
    fetchData();
  }, [fetchData]);

  // 刷新：发请求 + 延迟再拉取（让 LLM 任务有时间写库）
  const handleRefresh = useCallback(async () => {
    try {
      setRefreshing(true);
      await triggerRefresh(workspaceId);
      message.success('黑板刷新已触发，请稍后查看');
      // 延迟拉取：避免与触发请求竞争；用 setTimeout 即可，不需要后端推送
      window.setTimeout(fetchData, REFRESH_POLL_DELAY_MS);
    } catch (err) {
      console.error('刷新黑板失败:', err);
      message.error('刷新黑板失败');
    } finally {
      setRefreshing(false);
    }
  }, [workspaceId, fetchData]);

  // 是否为空：id=0 是后端"无记录"的占位
  const isEmpty = data === null || data.id === 0 || data.content.length === 0;

  return (
    <div style={{ padding: '16px 24px', height: '100%', overflow: 'auto' }}>
      <BlackboardHeader
        isDark={isDark}
        refreshing={refreshing}
        // 空状态时禁用刷新：避免无意义的 LLM 调用
        onRefresh={handleRefresh}
        disabled={isEmpty}
        onOpenSettings={handleOpenSettings}
      />
      <BlackboardBody isDark={isDark} data={data} />

      {/* 黑板设置弹窗 */}
      <Modal
        title="黑板设置"
        open={settingsOpen}
        onOk={handleSaveSettings}
        onCancel={() => setSettingsOpen(false)}
        okText="保存"
        confirmLoading={settingsSaving}
        destroyOnHidden
      >
        <Form layout="vertical">
          <Form.Item label="防抖周期">
            <InputNumber
              value={debounceSecs}
              onChange={(v) => setDebounceSecs(v ?? 600)}
              min={10}
              max={3600}
              addonAfter="秒"
              style={{ width: 200 }}
            />
          </Form.Item>
          <Form.Item label="触发条数">
            <InputNumber
              value={debounceCount}
              onChange={(v) => setDebounceCount(v ?? 10)}
              min={1}
              max={100}
              addonAfter="条"
              style={{ width: 200 }}
            />
          </Form.Item>
          <Form.Item extra="达到条数阈值或周期到期时，统一处理 pending 的 todo，减少频繁的 LLM 调用" />
        </Form>
      </Modal>
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

interface BlackboardHeaderProps {
  isDark: boolean;
  refreshing: boolean;
  onRefresh: () => void;
  disabled: boolean;
  onOpenSettings: () => void;
}

/** 顶部标题栏：标题 + 刷新按钮 + 设置按钮 */
function BlackboardHeader(props: BlackboardHeaderProps) {
  return (
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
      <Space.Compact>
        <Button
          icon={<SettingOutlined />}
          onClick={props.onOpenSettings}
          title="设置"
        />
        <Button
          type="primary"
          icon={<ReloadOutlined />}
          loading={props.refreshing}
          onClick={props.onRefresh}
          disabled={props.disabled}
        >
          {props.refreshing ? '更新中...' : '刷新'}
        </Button>
      </Space.Compact>
    </div>
  );
}

interface BlackboardBodyProps {
  isDark: boolean;
  data: BlackboardData | null;
}

/** 正文区：loading / 有内容 / 空状态三分支 */
function BlackboardBody(props: BlackboardBodyProps) {
  // 首次加载用 skeleton 提升感知性能；data 还没回来时显示占位
  if (props.data === null) {
    return <Skeleton active paragraph={{ rows: 8 }} />;
  }
  if (props.data.id === 0 || props.data.content.length === 0) {
    return <BlackboardEmpty isDark={props.isDark} />;
  }
  return <BlackboardContent isDark={props.isDark} content={props.data.content} />;
}

interface BlackboardContentProps {
  isDark: boolean;
  content: string;
}

/** 真正渲染 Markdown：XMarkdown 内部走 DOMPurify 防止 XSS */
function BlackboardContent(props: BlackboardContentProps) {
  const isDark = props.isDark;
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
        content={props.content}
        // 覆盖 a 标签渲染：让 ntd://todo/{id} 走内部导航
        components={{ a: TodoLink }}
        // DOMPurify 默认会拒绝 ntd:// 等未知协议，会把整条链接剥成纯文本。
        // 显式允许 ntd 协议，让内部链接得以保留；其它未知协议仍被拒绝。
        dompurifyConfig={{
          ALLOWED_URI_REGEXP: /^(?:(?:https?|mailto|tel|ntd):)/i,
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
