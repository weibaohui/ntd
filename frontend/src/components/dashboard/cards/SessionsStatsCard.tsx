// 「AI 会话」卡:扫描编辑器 JSONL 得到的会话数与 token。
// 与 ntd 自身 execution_records 是两条独立数据通道,互补呈现真实用量。
// Col 用响应式:移动端 2 列(xs:12)、桌面 3 列(sm:8),避免窄屏 3 列挤。
import { Statistic, Row, Col } from 'antd';
import { MessageOutlined } from '@ant-design/icons';
import { getSessionStats } from '@/utils/database/sessions';
import { useCardData } from '@/components/dashboard/useCardData';
import { CardShell } from './CardShell';

export function SessionsStatsCard() {
  const { data, loading, error } = useCardData(getSessionStats);
  return (
    <CardShell icon={<MessageOutlined />} title="AI 会话" loading={loading} error={error}>
      <Row gutter={[16, 12]}>
        <Col xs={12} sm={8}>
          <Statistic title="总会话" value={data?.total_sessions ?? 0} />
        </Col>
        <Col xs={12} sm={8}>
          <Statistic title="今日" value={data?.today_sessions ?? 0} />
        </Col>
        <Col xs={12} sm={8}>
          <Statistic title="活跃" value={data?.active_sessions ?? 0} />
        </Col>
        <Col xs={12} sm={12}>
          <Statistic title="输入 Token" value={data?.total_input_tokens ?? 0} />
        </Col>
        <Col xs={12} sm={12}>
          <Statistic title="输出 Token" value={data?.total_output_tokens ?? 0} />
        </Col>
      </Row>
    </CardShell>
  );
}
