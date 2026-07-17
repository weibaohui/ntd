// 「环路」卡:聚合所有 loop 的规模/成功率/触发器分布。
// 数据来自后端聚合端点 GET /api/loops/stats(一条 SQL,避免前端逐 loop 拉取的 N+1)。
// hours 来自全局时间范围,切换时重新拉取。Col 响应式:移动端 2 列、桌面 4 列。
import { Statistic, Row, Col, Tag } from 'antd';
import { RetweetOutlined } from '@ant-design/icons';
import { getLoopStats } from '@/utils/database/loops';
import { useCardData } from '@/components/dashboard/useCardData';
import { CardShell } from './CardShell';

// trigger_type 枚举值 → 中文,提升可读性;未知值原样回退。
const TRIGGER_LABEL: Record<string, string> = {
  manual: '手动',
  cron: '定时',
  webhook: 'Webhook',
  feishu_message: '飞书消息',
  feishu_command: '飞书命令',
  feishu_card: '飞书卡片',
  default_response: '默认响应',
  todo_completed: '事项完成',
  todo_state_changed: '状态变更',
  tag_added: '标签新增',
};

export function LoopStatsCard({ hours }: { hours?: number }) {
  const { data, loading, error } = useCardData(() => getLoopStats(hours), [hours]);
  // 成功率 = success / total;total=0 时归 0,避免除零。
  const successRate =
    data && data.total_executions > 0
      ? Math.round((data.success_executions / data.total_executions) * 100)
      : 0;
  return (
    <CardShell icon={<RetweetOutlined />} title="环路" loading={loading} error={error}>
      {data && (
        <>
          <Row gutter={[16, 12]}>
            <Col xs={12} sm={6}>
              <Statistic title="总数" value={data.total_loops} />
            </Col>
            <Col xs={12} sm={6}>
              <Statistic title="活跃" value={data.active_loops} />
            </Col>
            <Col xs={12} sm={6}>
              <Statistic title="执行次数" value={data.total_executions} />
            </Col>
            <Col xs={12} sm={6}>
              <Statistic title="成功率" value={successRate} suffix="%" />
            </Col>
          </Row>
          <div style={{ marginTop: 12 }}>
            <div style={{ fontSize: 12, color: 'var(--color-text-tertiary)', marginBottom: 6 }}>
              触发类型分布
            </div>
            {data.trigger_type_distribution.length === 0 ? (
              <span style={{ fontSize: 13, color: 'var(--color-text-tertiary)' }}>暂无执行</span>
            ) : (
              <div style={{ display: 'flex', flexWrap: 'wrap', gap: 6 }}>
                {data.trigger_type_distribution.map((t) => (
                  <Tag key={t.trigger_type}>
                    {TRIGGER_LABEL[t.trigger_type] ?? t.trigger_type}: {t.count}
                  </Tag>
                ))}
              </div>
            )}
          </div>
        </>
      )}
    </CardShell>
  );
}
