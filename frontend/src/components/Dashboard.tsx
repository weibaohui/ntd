// 仪表盘主页:负责加载数据 + 顶部全局时间范围 + 按 Tab 分发到各域卡片。
//
// 重构动机:此前把 24 张卡片塞进单个 Masonry 瀑布流,信息过载、关注域混杂、
// 新功能(Loop/自动化/资源盘点)无处安放。现拆成 6 个语义清晰的 Tab,
// 卡片组件本身 0 改动(纯展示),本组件只做数据装配与 Tab 容器编排。
//
// 移动端:Tab label 缩短(如「成本与模型」→「成本」),避免 6 个 Tab 在窄屏被
// antd 收进溢出下拉;时间范围与卡片内部布局由各组件自行响应式。
import { useEffect, useState, useCallback } from 'react';
import type { ComponentType, CSSProperties } from 'react';
import { Tabs, App } from 'antd';
import {
  DashboardOutlined,
  AppstoreOutlined,
  CheckSquareOutlined,
  ThunderboltOutlined,
  DollarOutlined,
  ClockCircleOutlined,
  ToolOutlined,
} from '@ant-design/icons';
import { PageCard } from '@/components/common/PageCard';
import dayjs from 'dayjs';
import { useApp } from '@/hooks/useApp';
import { useViewState } from '@/hooks/useViewState';
import { useIsMobile } from '@/hooks/useIsMobile';
import * as db from '@/utils/database';
import type { DashboardStats, FeishuMessageStats } from '@/types';
import { TimeRangeSelector } from './dashboard/SpecialCards';
import { OverviewTab } from './dashboard/tabs/OverviewTab';
import { TasksTab } from './dashboard/tabs/TasksTab';
import { ExecutionsTab } from './dashboard/tabs/ExecutionsTab';
import { CostTab } from './dashboard/tabs/CostTab';
import { AutomationTab } from './dashboard/tabs/AutomationTab';
import { ResourcesTab } from './dashboard/tabs/ResourcesTab';

// 全部合法 Tab key,顺序即展示顺序;as const 让 key 形成可校验的联合类型。
const DASHBOARD_TABS = ['overview', 'tasks', 'executions', 'cost', 'automation', 'resources'] as const;
type DashboardTabKey = (typeof DASHBOARD_TABS)[number];

// Tab 图标类型(ant-design 图标组件)。
type IconType = ComponentType<{ style?: CSSProperties }>;

export function Dashboard() {
  const { state } = useApp();
  const { message } = App.useApp();
  const { todos, tags, runningTasks } = state;
  const { activeTab, pushUrl } = useViewState();
  const isMobile = useIsMobile();

  const [stats, setStats] = useState<DashboardStats | null>(null);
  const [loading, setLoading] = useState(true);
  const [msgStats, setMsgStats] = useState<FeishuMessageStats | null>(null);
  const [msgStatsError, setMsgStatsError] = useState(false);
  const [timeRange, setTimeRange] = useState<number | 'custom'>(720);
  const [customRange, setCustomRange] = useState<[dayjs.Dayjs, dayjs.Dayjs] | null>(null);
  // ccusage 通道(UsageStatsCard 自取数据)的时间窗,由顶部 TimeRangeSelector 派生。
  const [usageStatsRange, setUsageStatsRange] = useState<{ since?: string; until?: string }>({});

  // 顶层派生量:多处 Tab 共用,在此算一次后通过 props 下发,避免各 Tab 重复计算。
  const totalTodos = stats?.total_todos ?? todos.length;
  const successRate =
    stats && stats.total_executions > 0 ? (stats.success_executions / stats.total_executions) * 100 : 0;
  const processingRate =
    msgStats && msgStats.total_messages > 0 ? (msgStats.processed / msgStats.total_messages) * 100 : 0;
  // 当前时间范围换算成小时数,供自动化 Tab 的 Loop 聚合按窗口过滤;custom 时 undefined=全时段。
  const currentHours = typeof timeRange === 'number' ? timeRange : undefined;

  const loadStats = async (hours?: number) => {
    try {
      setLoading(true);
      const data = await db.getDashboardStats(hours);
      setStats(data);
    } catch {
      message.error('加载统计数据失败');
    } finally {
      setLoading(false);
    }
  };

  const loadMsgStats = async (hours?: number) => {
    try {
      setMsgStatsError(false);
      const data = await db.getFeishuMessageStats(hours);
      setMsgStats(data);
    } catch {
      // 飞书未配置时该端点会失败,用布尔标记降级展示,不弹错误打扰用户。
      setMsgStatsError(true);
    }
  };

  // 时间范围切换:把所选小时数同时喂给 dashboard 聚合 stats 与飞书消息 stats,
  // 并派生 ccusage 通道的 since/until ISO 串(UsageStatsCard 自取数据时用)。
  const handleTimeRangeChange = (value: number | 'custom') => {
    setTimeRange(value);
    if (value === 'custom') {
      // 自定义模式:等用户选完日期区间再触发加载,避免中间态的无谓请求。
      return;
    }
    setCustomRange(null);
    setUsageStatsRange({
      until: new Date().toISOString(),
      since: new Date(Date.now() - value * 60 * 60 * 1000).toISOString(),
    });
    loadStats(value);
    loadMsgStats(value);
  };

  const handleCustomRangeChange = (dates: [dayjs.Dayjs, dayjs.Dayjs] | null) => {
    setCustomRange(dates);
    if (!dates) return;
    setUsageStatsRange({
      since: dates[0].toISOString(),
      until: dates[1].toISOString(),
    });
    // 自定义区间换算成小时数,复用同一套按小时过滤的加载逻辑。
    const hours = Math.round(dates[1].diff(dates[0], 'hour', true));
    loadStats(hours);
    loadMsgStats(hours);
  };

  // 首次挂载默认 30 天(720h),与重构前行为一致,避免改变用户既有的时间窗预期。
  useEffect(() => {
    loadStats(720);
    loadMsgStats(720);
    setUsageStatsRange({
      until: new Date().toISOString(),
      since: new Date(Date.now() - 720 * 60 * 60 * 1000).toISOString(),
    });
    // 仅初始化一次,故空依赖数组;loadStats/loadMsgStats 是组件内闭包,无需进依赖。
  }, []);

  // URL hash 的 tab 参数校验:非法/缺失值回退 overview,
  // 保证刷新或分享链接始终落在有效 Tab,不会渲染空白。
  const resolvedTab: DashboardTabKey = DASHBOARD_TABS.includes(activeTab as DashboardTabKey)
    ? (activeTab as DashboardTabKey)
    : 'overview';

  // Tab 切换写入 URL(而非仅本地 state),让浏览器前进/后退、刷新都保持当前 Tab。
  const handleTabChange = useCallback(
    (key: string) => {
      pushUrl('dashboard', { tab: key });
    },
    [pushUrl],
  );

  // Tab label:移动端用短文案(避免 6 个 Tab 在窄屏溢出被收进下拉),桌面用完整文案。
  const renderLabel = (Icon: IconType, full: string, short: string) => (
    <span>
      <Icon style={{ marginRight: 6 }} />
      {isMobile ? short : full}
    </span>
  );

  // Tab 配置:label 带图标增强辨识;children 渲染对应 Tab 组件并下发所需数据。
  const tabItems = [
    {
      key: 'overview',
      label: renderLabel(AppstoreOutlined, '总览', '总览'),
      children: (
        <OverviewTab
          stats={stats}
          loading={loading}
          successRate={successRate}
          runningTasks={Object.values(runningTasks)}
          todos={todos}
        />
      ),
    },
    {
      key: 'tasks',
      label: renderLabel(CheckSquareOutlined, '任务', '任务'),
      children: <TasksTab stats={stats} loading={loading} totalTodos={totalTodos} />,
    },
    {
      key: 'executions',
      label: renderLabel(ThunderboltOutlined, '执行', '执行'),
      children: <ExecutionsTab stats={stats} loading={loading} totalTodos={totalTodos} tagsLength={tags.length} />,
    },
    {
      key: 'cost',
      label: renderLabel(DollarOutlined, '成本与模型', '成本'),
      children: <CostTab stats={stats} loading={loading} usageSince={usageStatsRange.since} usageUntil={usageStatsRange.until} />,
    },
    {
      key: 'automation',
      label: renderLabel(ClockCircleOutlined, '自动化', '自动化'),
      children: <AutomationTab msgStats={msgStats} msgStatsError={msgStatsError} processingRate={processingRate} hours={currentHours} />,
    },
    {
      key: 'resources',
      label: renderLabel(ToolOutlined, '资源与运维', '资源'),
      children: <ResourcesTab stats={stats} loading={loading} />,
    },
  ];

  return (
    <PageCard icon={<DashboardOutlined />} title="仪表盘">
      <div style={{ padding: '16px 20px', background: 'var(--color-bg-elevated)' }}>
        <style>{`
          .dashboard-card { transition: border-color 0.2s, box-shadow 0.2s; }
          .dashboard-card:hover { border-color: var(--color-border); box-shadow: 0 2px 12px rgba(0,0,0,0.08); }
        `}</style>
        {/* 时间范围全局共享:所有 Tab 看同一时间窗,切换 Tab 不丢失筛选上下文。 */}
        <TimeRangeSelector
          timeRange={timeRange}
          customRange={customRange}
          onTimeRangeChange={handleTimeRangeChange}
          onCustomRangeChange={handleCustomRangeChange}
        />
        <Tabs
          className="dashboard-tabs"
          items={tabItems}
          type="card"
          size="small"
          activeKey={resolvedTab}
          onChange={handleTabChange}
          style={{ marginTop: 16 }}
        />
      </div>
    </PageCard>
  );
}
