// 「智能助手 Bot」卡:飞书 Bot 数量与启用状态。
import { Statistic, Row, Col } from 'antd';
import { RobotOutlined } from '@ant-design/icons';
import { getAgentBots } from '@/utils/database/bots';
import { useCardData } from '../useCardData';
import { CardShell } from './CardShell';

export function AgentBotsCard() {
  const { data, loading, error } = useCardData(getAgentBots);
  const bots = data ?? [];
  const enabled = bots.filter((b) => b.enabled).length;
  return (
    <CardShell icon={<RobotOutlined />} title="智能助手" loading={loading} error={error}>
      <Row gutter={16}>
        <Col span={12}>
          <Statistic title="Bot 总数" value={bots.length} />
        </Col>
        <Col span={12}>
          <Statistic title="已启用" value={enabled} />
        </Col>
      </Row>
    </CardShell>
  );
}
