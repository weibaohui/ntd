// 概念导航首页主容器。
// PageCard 外壳 + Hero 区 + sticky Tab + 3 个 section（关系图/概念详解/快速开始）。
// IntersectionObserver 监听 section 滚动，自动高亮 sticky Tab。
// 首次进入检测 localStorage['ntd_onboarding_completed']，无标记则本页展示；点「跳过引导」写标记 + 跳仪表盘。

import { useEffect, useRef, useState, useCallback } from 'react';
import { Button, Tabs, Typography } from 'antd';
import { CompassOutlined, ForwardOutlined } from '@ant-design/icons';
import { PageCard } from '@/components/common/PageCard';
import { useViewState } from '@/hooks/useViewState';
import { ConceptRelationGraph } from '@/components/onboarding/ConceptRelationGraph';
import { ConceptCardGrid } from '@/components/onboarding/ConceptCardGrid';
import { ConceptDetailSection } from '@/components/onboarding/ConceptDetailSection';
import { QuickStartFlow } from '@/components/onboarding/QuickStartFlow';
import {
  CONCEPTS,
  ONBOARDING_TABS,
  ONBOARDING_HERO_TITLE,
  ONBOARDING_HERO_SUBTITLE,
} from '@/components/onboarding/concepts';

const { Title, Paragraph, Text } = Typography;

interface ConceptNavPageProps {
  /** 当前工作空间 id，透传给数据拉取子组件。 */
  workspaceId: number | null;
}

/** sticky Tab 的三个 key，与 section id 对齐。 */
type TabKey = 'relation' | 'concepts' | 'quickstart';

/** 写入「跳过引导」标记到 localStorage，包裹 try/catch 静默降级。 */
function persistOnboardingSkipped() {
  try {
    localStorage.setItem('ntd_onboarding_completed', 'true');
  } catch {
    /* localStorage 不可用时静默降级，不阻塞跳转 */
  }
}

/** Hero 区：大标题 + 一句话简介 + 跳过引导按钮。 */
function Hero({ onSkip }: { onSkip: () => void }) {
  return (
    <div
      style={{
        // 胶物式背景：轻渐变 + 圆角，与 Hero 区视觉重心。
        background: 'linear-gradient(135deg, #1677ff 0%, #722ed1 100%)',
        borderRadius: 'var(--radius-md, 8px)',
        padding: '32px 24px',
        marginBottom: 24,
        color: '#fff',
      }}
      data-testid="onboarding-hero"
    >
      <Title level={2} style={{ color: '#fff', marginTop: 0, marginBottom: 8 }}>
        {ONBOARDING_HERO_TITLE}
      </Title>
      <Paragraph style={{ color: 'rgba(255, 255, 255, 0.85)', marginBottom: 16, fontSize: 15 }}>
        {ONBOARDING_HERO_SUBTITLE}
      </Paragraph>
      <Button
        // 胶物按钮：白底，与渐变背景对比清晰。
        style={{ background: '#fff', color: '#1677ff', border: 'none', fontWeight: 500 }}
        icon={<ForwardOutlined />}
        onClick={onSkip}
        data-testid="onboarding-skip-btn"
      >
        跳过引导
      </Button>
    </div>
  );
}

/**
 * 概念导航首页主容器。
 *
 * 整体处理思路：
 * 1. PageCard 外壳，与 app 其他页一致。
 * 2. Hero 区置顶，讲一句话定位 + 跳过引导。
 * 3. sticky Tab 三个 key，点击平滑滚动到对应 section。
 * 4. IntersectionObserver 监听 section 进入视口，自动高亮 Tab。
 * 5. 三个 section：关系图 / 概念详解（卡片网格 + 6 个详细段） / 快速开始。
 */
export function ConceptNavPage({ workspaceId }: ConceptNavPageProps) {
  const { replaceUrl } = useViewState();

  // sticky Tab 当前高亮 key。
  // 默认 'relation'（首屏可见的是关系图）。
  const [activeTab, setActiveTab] = useState<TabKey>('relation');

  // 三个 section 的 ref，用于 IntersectionObserver 监听 + Tab 点击 scrollIntoView。
  // 用 HTMLElement 兜底类型，初始 null 与 RefObject 兼容。
  const relationRef = useRef<HTMLElement | null>(null);
  const conceptsRef = useRef<HTMLElement | null>(null);
  const quickstartRef = useRef<HTMLElement | null>(null);

  // 跳过引导：写 localStorage + replaceUrl 路由跳转。
  // replaceUrl 避免污染历史栈，用户点返回不会回到 onboarding。
  const handleSkip = useCallback(() => {
    persistOnboardingSkipped();
    replaceUrl('dashboard', {});
  }, [replaceUrl]);

  // sticky Tab 点击 → 平滑滚动到对应 section。
  const handleTabChange = useCallback((key: string) => {
    setActiveTab(key as TabKey);
    // 取对应 ref 的 current（HTMLElement | null），null 时静默降级。
    const targetEl: HTMLElement | null =
      key === 'relation' ? relationRef.current :
      key === 'concepts' ? conceptsRef.current :
      key === 'quickstart' ? quickstartRef.current : null;
    if (targetEl) {
      targetEl.scrollIntoView({ behavior: 'smooth', block: 'start' });
    }
  }, []);

  // IntersectionObserver 监听 section 滚动位置，自动高亮 Tab。
  // 依赖 activeTab 不入 effect 依赖，避免回写 activeTab 触发循环。
  useEffect(() => {
    const sections = [
      { key: 'relation' as TabKey, el: relationRef.current },
      { key: 'concepts' as TabKey, el: conceptsRef.current },
      { key: 'quickstart' as TabKey, el: quickstartRef.current },
    ].filter((s): s is { key: TabKey; el: HTMLElement } => s.el != null);

    if (sections.length === 0) return;

    // 用 rootMargin 让 section 顶部进入视口上 1/3 时就算激活，避免 Tab 切换滞后。
    const observer = new IntersectionObserver(
      (entries) => {
        // 找当前与视口相交比例最大的 section。
        let best: { key: TabKey; ratio: number } | null = null;
        for (const entry of entries) {
          if (entry.isIntersecting) {
            const matched = sections.find((s) => s.el === entry.target);
            if (matched && (!best || entry.intersectionRatio > best.ratio)) {
              best = { key: matched.key, ratio: entry.intersectionRatio };
            }
          }
        }
        if (best) {
          setActiveTab(best.key);
        }
      },
      { threshold: [0, 0.25, 0.5, 0.75, 1], rootMargin: '-20% 0px -60% 0px' },
    );

    for (const s of sections) {
      observer.observe(s.el);
    }
    return () => observer.disconnect();
  }, []);

  return (
    <PageCard
      icon={<CompassOutlined />}
      title="NTD 概念导航"
      style={{ flex: 1, height: '100%' }}
      // 内容区高度撑满并支持内部滚动（sticky Tab 需粘在内容区顶部）。
      contentStyle={{ height: 'calc(100% - 43px)', overflow: 'auto', position: 'relative' }}
    >
      {/* Hero 区 */}
      <Hero onSkip={handleSkip} />

      {/* sticky Tab：粘在内容区顶部，滚动时自动高亮 */}
      <div
        style={{
          position: 'sticky',
          top: 0,
          zIndex: 10,
          background: 'var(--color-bg-card, #fff)',
          borderBottom: '1px solid var(--color-border-light, #f0f0f0)',
          paddingTop: 8,
          marginBottom: 16,
        }}
        data-testid="onboarding-sticky-tab"
      >
        <Tabs
          activeKey={activeTab}
          onChange={handleTabChange}
          items={ONBOARDING_TABS.map((t) => ({
            key: t.key,
            label: (
              <span>
                {t.icon}
                <span style={{ marginLeft: 4 }}>{t.label}</span>
              </span>
            ),
          }))}
          // size="small" 让 sticky Tab 更轻量，不占用太多垂直空间。
          size="small"
          // tabBarStyle 让 Tabs 自身也无额外边距，紧贴 sticky 容器。
          tabBarStyle={{ marginBottom: 0 }}
        />
      </div>

      {/* section 1：关系图 */}
      <section
        ref={relationRef}
        id="relation"
        // ref 需是 HTMLElement，section 标签本身即 HTMLElement，挂 ref 时不需特殊处理。
        // 但 React18 的 ref typing 对 section 偏严，这里用 HTMLElement 而非 HTMLDivElement。
        style={{ scrollMarginTop: 60, paddingBottom: 48 }}
      >
        <Title level={3}>
          <CompassOutlined style={{ marginRight: 8 }} />
          概念关系图
        </Title>
        <Text type="secondary" style={{ display: 'block', marginBottom: 16 }}>
          点击任一节点查看详情，hover 高亮关联概念。流向：工艺 → 环路 → 事项 → 执行记录。
        </Text>
        <ConceptRelationGraph />
      </section>

      {/* section 2：概念详解（卡片网格 + 6 个详细说明段） */}
      <section
        ref={conceptsRef}
        id="concepts"
        style={{ scrollMarginTop: 60, paddingBottom: 48 }}
      >
        <Title level={3}>概念详解</Title>
        <Text type="secondary" style={{ display: 'block', marginBottom: 16 }}>
          6 个核心概念逐个展开，含字段定义 + 你系统里的真实数据快照。
        </Text>
        <ConceptCardGrid workspaceId={workspaceId} />
        {/* 6 个详细说明段：ConceptDetailSection 内部已含 section 锚点 */}
        {CONCEPTS.map((concept) => (
          <ConceptDetailSection
            key={concept.id}
            concept={concept}
            workspaceId={workspaceId}
          />
        ))}
      </section>

      {/* section 3：快速开始 */}
      <section
        ref={quickstartRef}
        id="quickstart"
        style={{ scrollMarginTop: 60, paddingBottom: 48 }}
      >
        <Title level={3}>快速开始</Title>
        <Text type="secondary" style={{ display: 'block', marginBottom: 16 }}>
          按顺序完成下面 5 步，即可跑通你的第一个 AI 任务。
        </Text>
        <QuickStartFlow workspaceId={workspaceId} />
      </section>
    </PageCard>
  );
}
