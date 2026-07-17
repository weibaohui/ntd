// 「模板计数」卡:统计事项模板与评审模板数量,反映自动化配置规模。
// 两个模板源并发拉取后合并为一次加载,减少视觉抖动。
import { Statistic, Row, Col } from 'antd';
import { FileTextOutlined } from '@ant-design/icons';
import { getTodoTemplates } from '@/utils/database/todos';
import { listReviewTemplates } from '@/utils/database/reviewTemplates';
import { useCardData } from '../useCardData';
import { CardShell } from './CardShell';

interface TemplateCounts {
  todoCount: number;
  reviewCount: number;
}

async function loadCounts(): Promise<TemplateCounts> {
  const [todos, reviews] = await Promise.all([getTodoTemplates(), listReviewTemplates()]);
  return { todoCount: todos.length, reviewCount: reviews.length };
}

export function TemplateCountCard() {
  const { data, loading, error } = useCardData(loadCounts);
  return (
    <CardShell icon={<FileTextOutlined />} title="模板" loading={loading} error={error}>
      <Row gutter={16}>
        <Col span={12}>
          <Statistic title="事项模板" value={data?.todoCount ?? 0} />
        </Col>
        <Col span={12}>
          <Statistic title="评审模板" value={data?.reviewCount ?? 0} />
        </Col>
      </Row>
    </CardShell>
  );
}
