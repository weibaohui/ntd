// 「专家」卡:专家/专家团数量 + 团队成员(assistant)总数。
import { Statistic, Row, Col } from 'antd';
import { TeamOutlined } from '@ant-design/icons';
import { getAllExperts } from '@/utils/database/experts';
import { useCardData } from '@/components/dashboard/useCardData';
import { CardShell } from './CardShell';

export function ExpertsCard() {
  const { data, loading, error } = useCardData(getAllExperts);
  const experts = data ?? [];
  // team 型专家的成员累加(agent 型为空数组),反映 assistant 级总数。
  const totalMembers = experts.reduce((sum, e) => sum + (e.members?.length ?? 0), 0);
  return (
    <CardShell icon={<TeamOutlined />} title="专家" loading={loading} error={error}>
      <Row gutter={16}>
        <Col span={12}>
          <Statistic title="专家/团队" value={experts.length} />
        </Col>
        <Col span={12}>
          <Statistic title="团队成员" value={totalMembers} />
        </Col>
      </Row>
    </CardShell>
  );
}
