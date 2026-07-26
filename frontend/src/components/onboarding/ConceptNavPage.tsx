// 概念导航首页主容器。
// PageCard 外壳 + Hero 区 + 2 个 section（关系图/概念详解）。

import { Typography } from 'antd';
import { CompassOutlined } from '@ant-design/icons';
import { PageCard } from '@/components/common/PageCard';
import { ConceptRelationGraph } from '@/components/onboarding/ConceptRelationGraph';
import { ConceptCardGrid } from '@/components/onboarding/ConceptCardGrid';
import { ConceptDetailSection } from '@/components/onboarding/ConceptDetailSection';
import {
  CONCEPTS,
  ONBOARDING_HERO_TITLE,
  ONBOARDING_HERO_SUBTITLE,
} from '@/components/onboarding/concepts';

const { Title, Paragraph, Text } = Typography;

interface ConceptNavPageProps {
  /** 当前工作空间 id，透传给数据拉取子组件。 */
  workspaceId: number | null;
}

/** Hero 区：大标题 + 一句话简介。 */
function Hero() {
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
      <Paragraph style={{ color: 'rgba(255, 255, 255, 0.85)', marginBottom: 0, fontSize: 15 }}>
        {ONBOARDING_HERO_SUBTITLE}
      </Paragraph>
    </div>
  );
}

/**
 * 概念导航首页主容器。
 *
 * 整体处理思路：
 * 1. PageCard 外壳，与 app 其他页一致。
 * 2. Hero 区置顶，讲一句话定位。
 * 3. 两个 section：关系图 / 概念详解（卡片网格 + 6 个详细段）。
 */
export function ConceptNavPage({ workspaceId }: ConceptNavPageProps) {
  return (
    <PageCard
      icon={<CompassOutlined />}
      title="NTD 概念导航"
      style={{ flex: 1, height: '100%' }}
      contentStyle={{ height: 'calc(100% - 43px)', overflow: 'auto', position: 'relative' }}
    >
      {/* Hero 区 */}
      <Hero />

      {/* section 1：关系图 */}
      <section
        id="relation"
        style={{ paddingBottom: 48 }}
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
        id="concepts"
        style={{ paddingBottom: 48 }}
      >
        <Title level={3}>概念详解</Title>
        <Text type="secondary" style={{ display: 'block', marginBottom: 16 }}>
          6 个核心概念逐个展开，含字段定义。
        </Text>
        <ConceptCardGrid workspaceId={workspaceId} />
        {/* 6 个详细说明段：ConceptDetailSection 内部已含 section 锚点 */}
        {CONCEPTS.map((concept) => (
          <ConceptDetailSection
            key={concept.id}
            concept={concept}
          />
        ))}
      </section>
    </PageCard>
  );
}
