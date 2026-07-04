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
import { Button, Typography, Skeleton, message, Modal, Form, InputNumber, Space, Progress, Input, Tabs } from 'antd';
import { ReloadOutlined, SettingOutlined } from '@ant-design/icons';
import { TfiBlackboard } from 'react-icons/tfi';
import { XMarkdown } from '@ant-design/x-markdown';
import { useTheme } from '@/hooks/useTheme';
import { useViewState } from '@/hooks/useViewState';
import type { BlackboardDebounceStatus } from '@/hooks/useExecutionEvents';
import { updateBlackboardConfig } from '@/utils/database/blackboard';

const { Title } = Typography;

/** 黑板 API 返回的 JSON 形状（与后端 BlackboardResponse 对应） */
interface BlackboardData {
  id: number;
  workspace_id: number;
  content: string;
  updated_at: string | null;
  /** 黑板更新防抖周期（秒）*/
  blackboard_debounce_secs: number;
  /** 黑板更新防抖条数阈值 */
  blackboard_debounce_count: number;
  /** 黑板更新提示词模板（空字符串表示使用内置默认）*/
  blackboard_update_prompt: string;
  /** 黑板刷新提示词模板（空字符串表示使用内置默认）*/
  blackboard_refresh_prompt: string;
}

/** ntd://todo/{id} 协议的前缀，用于解析 LLM 注入的内部链接 */
const NTD_TODO_PROTOCOL_PREFIX = 'ntd://todo/';

/** URL search 参数 `workspace` 的键名 */
const URL_WORKSPACE_PARAM = 'workspace';

/** 默认工作空间 ID（首屏兜底，避免 URL 未带参时无 workspace） */
const DEFAULT_WORKSPACE_ID = 1;

/**
 * 黑板更新提示词默认值，与后端 DEFAULT_BLACKBOARD_UPDATE_PROMPT 保持一致。
 * 用于在 UI 上展示默认提示词内容，以及"恢复默认"时回填。
 */
const DEFAULT_BLACKBOARD_UPDATE_PROMPT = `你是一个工作空间知识库的维护者。你的任务是维护一个 Markdown 格式的"黑板"，记录工作空间中所有任务执行的结论和当前进展。

当前黑板内容：
\`\`\`
{{current}}
\`\`\`

新任务结论：
- 任务 ID: {{todo_id}}
- 任务标题: {{todo_title}}
- 执行结论: {{conclusion}}

请更新黑板内容，要求：
1. 将新结论整合到黑板中
2. 保持以下结构：
   - # 工作空间进展
   - ## 已确认
   - ## 新发现
   - ## 待解决问题
   - ## 矛盾/风险
   - ## 下一步建议
3. 每条结论标注来源，格式：(来源: [todo_{{todo_id}}](ntd://todo/{{todo_id}}))
4. 如果新结论与已有结论矛盾，在"矛盾/风险"中标注
5. 如果新结论提出了未解决的问题，在"待解决问题"中列出
6. 更新"下一步建议"
7. 保持 Markdown 格式，不要添加 HTML
8. 如果黑板为空，根据新结论创建初始结构

只输出更新后的黑板内容，不要输出任何解释。`;


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


export function BlackboardPage({ workspaceId: propWorkspaceId }: { workspaceId?: number | null }) {
  // 主题：决定黑板容器背景与文字色
  const { themeMode } = useTheme();
  const isDark = themeMode === 'dark';
  // 派生值（不再 useState）：切换工作空间时自动跟随 prop 变化
  const workspaceId = useEffectiveWorkspaceId(propWorkspaceId);

  // 数据状态：data 既是"内容"也是"是否加载完成"的判断
  const [data, setData] = useStateBlackboardData();
  // 设置弹窗状态
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [settingsSaving, setSettingsSaving] = useState(false);
  const [debounceSecs, setDebounceSecs] = useState<number>(600);
  const [debounceCount, setDebounceCount] = useState<number>(10);
  const [updatePrompt, setUpdatePrompt] = useState<string>('');
  const [activeTab, setActiveTab] = useState<'debounce' | 'prompt'>('debounce');

  /**
   * 打开设置弹窗：从已加载的黑板数据中读取 per-workspace 配置。
   * 配置现在由 GET /api/workspaces/{workspaceId}/blackboard 接口随内容一并返回，
   * 不再需要单独调用 db.getConfig()（getConfig 是全局配置，与黑板配置无关）。
   */
  const handleOpenSettings = useCallback(() => {
    if (data) {
      setDebounceSecs(data.blackboard_debounce_secs ?? 600);
      setDebounceCount(data.blackboard_debounce_count ?? 10);
      setUpdatePrompt(data.blackboard_update_prompt ?? '');
    } else {
      setDebounceSecs(600);
      setDebounceCount(10);
      setUpdatePrompt('');
    }
    setActiveTab('debounce');
    setSettingsOpen(true);
  }, [data]);

  // 保存设置
  const handleSaveSettings = useCallback(async () => {
    setSettingsSaving(true);
    try {
      await updateBlackboardConfig(workspaceId, {
        blackboard_debounce_secs: debounceSecs,
        blackboard_debounce_count: debounceCount,
        blackboard_update_prompt: updatePrompt,
      });
      message.success('设置已保存');
      setSettingsOpen(false);
    } catch (err) {
      message.error('保存失败: ' + (err instanceof Error ? err.message : String(err)));
    } finally {
      setSettingsSaving(false);
    }
  }, [workspaceId, debounceSecs, debounceCount, updatePrompt]);

  /**
   * 恢复默认提示词：把 updatePrompt 设为 DEFAULT_BLACKBOARD_UPDATE_PROMPT（与后端内置一致）。
   * 写入后端后，backend blackboard_update_prompt 为非空字符串，不再走 build_blackboard_prompt() 内置逻辑。
   * 区别于"留空"的语义——留空表示后端使用内置默认；填入默认值表示用户显式采用内置模板。
   */
  const handleRestoreDefaultPrompt = useCallback(() => {
    setUpdatePrompt(DEFAULT_BLACKBOARD_UPDATE_PROMPT);
  }, []);

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

  // 刷新：页面 reload，重新拉取最新内容
  const handleRefresh = useCallback(() => {
    window.location.reload();
  }, []);

  return (
    <div style={{ padding: '16px 24px', height: '100%', overflow: 'auto' }}>
      <BlackboardHeader
        isDark={isDark}
        onRefresh={handleRefresh}
        onOpenSettings={handleOpenSettings}
        workspaceId={workspaceId}
      />
      <BlackboardBody isDark={isDark} data={data} />

      {/* 黑板设置弹窗：Tab1 防抖设置，Tab2 提示词设置 */}
      <Modal
        title="黑板设置"
        open={settingsOpen}
        onOk={handleSaveSettings}
        onCancel={() => setSettingsOpen(false)}
        okText="保存"
        confirmLoading={settingsSaving}
        destroyOnHidden
        width={640}
      >
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
                />
              ),
            },
            {
              key: 'prompt',
              label: '提示词设置',
              children: (
                <PromptSettingsTab
                  updatePrompt={updatePrompt}
                  setUpdatePrompt={setUpdatePrompt}
                  onRestoreDefault={handleRestoreDefaultPrompt}
                />
              ),
            },
          ]}
        />
      </Modal>
    </div>
  );
}

// ─── 设置弹窗子组件（避免 Tabs children 深层嵌套）─────────────────

interface DebounceSettingsTabProps {
  debounceSecs: number;
  setDebounceSecs: (v: number) => void;
  debounceCount: number;
  setDebounceCount: (v: number) => void;
}

/** 防抖设置 Tab：防抖周期 + 触发条数，受父组件状态控制 */
function DebounceSettingsTab({ debounceSecs, setDebounceSecs, debounceCount, setDebounceCount }: DebounceSettingsTabProps) {
  return (
    <Form layout="vertical" style={{ marginTop: 16 }}>
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
  );
}

interface PromptSettingsTabProps {
  updatePrompt: string;
  setUpdatePrompt: (v: string) => void;
  onRestoreDefault: () => void;
}

/** 提示词设置 Tab：TextArea 输入自定义提示词 + 恢复默认按钮 */
function PromptSettingsTab({ updatePrompt, setUpdatePrompt, onRestoreDefault }: PromptSettingsTabProps) {
  return (
    <div style={{ marginTop: 16 }}>
      <Space style={{ marginBottom: 12 }}>
        <Button onClick={onRestoreDefault}>恢复默认</Button>
        <span style={{ color: '#888', fontSize: 12 }}>
          点击将内置默认提示词填入输入框，可继续编辑
        </span>
      </Space>
      <Input.TextArea
        value={updatePrompt}
        onChange={(e) => setUpdatePrompt(e.target.value)}
        rows={16}
        placeholder="留空使用内置默认提示词，如需自定义请直接在此输入"
      />
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
  onRefresh: () => void;
  onOpenSettings: () => void;
  workspaceId: number;
}

/** 顶部标题栏：标题 + 倒计时进度条 + 刷新按钮 + 设置按钮 */
function BlackboardHeader(props: BlackboardHeaderProps) {
  return (
    <div
      style={{
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'space-between',
        marginBottom: 16,
        gap: 12,
      }}
    >
      <Space style={{ margin: 0, flexShrink: 0 }}>
        <TfiBlackboard style={{ fontSize: 20, verticalAlign: 'middle' }} />
        <Title level={4} style={{ margin: 0 }}>
          黑板
        </Title>
      </Space>
      {/* 双进度条倒计时（自动监听 WebSocket 事件） */}
      <BlackboardDebounceBar workspaceId={props.workspaceId} />
      <Space.Compact>
        <Button
          icon={<SettingOutlined />}
          onClick={props.onOpenSettings}
          title="设置"
        />
        <Button
          type="primary"
          icon={<ReloadOutlined />}
          onClick={props.onRefresh}
        >
          刷新
        </Button>
      </Space.Compact>
    </div>
  );
}

// ─── 黑板倒计时进度条 ───────────────────────────────────────────

interface BlackboardDebounceBarProps {
  /** 当前工作空间 ID，用于过滤事件 */
  workspaceId: number;
  /** 刷新状态回调（正在刷新时禁用手动刷新按钮） */
  onRefreshing?: (v: boolean) => void;
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
