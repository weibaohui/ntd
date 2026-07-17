// 「飞书监听」卡:展示监听的会话数、启用数、最后抓取时间。
// 与 MessageStatsCard 互补:后者看消息吞吐量,本卡看监听配置的运行健康度。
// Col 响应式:移动端 2 列、桌面 3 列。
import { Statistic, Row, Col } from 'antd';
import { EyeOutlined } from '@ant-design/icons';
import { getFeishuHistoryChats } from '@/utils/database/bots';
import { formatRelativeTime } from '@/utils/datetime';
import { useCardData } from '../useCardData';
import { CardShell } from './CardShell';

// 取所有 chat 里最新的 last_fetch_time(字符串字典序即时间序)。
// 用 sort + 末位索引而非 Array.at(-1),兼容更低 ES target。
function latestFetchTime(times: (string | null)[]): string | undefined {
  const sorted = times.filter((t): t is string => Boolean(t)).sort();
  return sorted.length > 0 ? sorted[sorted.length - 1] : undefined;
}

export function FeishuMonitorCard() {
  const { data, loading, error } = useCardData(getFeishuHistoryChats);
  const chats = data ?? [];
  const enabled = chats.filter((c) => c.enabled).length;
  const lastFetch = latestFetchTime(chats.map((c) => c.last_fetch_time));
  return (
    <CardShell icon={<EyeOutlined />} title="飞书监听" loading={loading} error={error}>
      <Row gutter={[16, 12]}>
        <Col xs={12} sm={8}>
          <Statistic title="监听会话" value={chats.length} />
        </Col>
        <Col xs={12} sm={8}>
          <Statistic title="已启用" value={enabled} />
        </Col>
        <Col xs={24} sm={8}>
          <Statistic title="最后抓取" value={lastFetch ? formatRelativeTime(lastFetch) : '-'} />
        </Col>
      </Row>
    </CardShell>
  );
}
