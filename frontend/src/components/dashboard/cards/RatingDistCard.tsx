// 「自动评审评分分布」卡:统计最近完成 todo 的 rating 分布。
// rating 为 0-100;分四桶:优秀(≥80)、良好(60-79)、待改进(<60)、未评分。
import { Progress } from 'antd';
import { StarOutlined } from '@ant-design/icons';
import { getRecentCompletedTodos } from '@/utils/database/executions';
import { useCardData } from '@/components/dashboard/useCardData';
import { CardShell } from './CardShell';

interface RatingBuckets {
  excellent: number;
  good: number;
  poor: number;
  unscored: number;
  total: number;
}

// 把 rating 列表分桶;null/undefined 归「未评分」。total 用原始长度(非桶和)。
function bucketize(ratings: (number | null | undefined)[]): RatingBuckets {
  return {
    excellent: ratings.filter((r) => r != null && r >= 80).length,
    good: ratings.filter((r) => r != null && r >= 60 && r < 80).length,
    poor: ratings.filter((r) => r != null && r < 60).length,
    unscored: ratings.filter((r) => r == null).length,
    total: ratings.length,
  };
}

interface RatingBarProps {
  label: string;
  count: number;
  total: number;
  color: string;
}

// 单条分桶的标签 + 占比进度条。total=0 时 percent 归 0,避免 NaN。
function RatingBar({ label, count, total, color }: RatingBarProps) {
  const percent = total > 0 ? Math.round((count / total) * 100) : 0;
  return (
    <div style={{ marginBottom: 4 }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', fontSize: 13 }}>
        <span>{label}</span>
        <span>
          {count} ({percent}%)
        </span>
      </div>
      <Progress percent={percent} strokeColor={color} showInfo={false} size="small" />
    </div>
  );
}

export function RatingDistCard() {
  const { data, loading, error } = useCardData(() => getRecentCompletedTodos());
  const buckets = bucketize((data ?? []).map((t) => t.rating));
  return (
    <CardShell icon={<StarOutlined />} title="评分分布" loading={loading} error={error}>
      <RatingBar label="优秀(≥80)" count={buckets.excellent} total={buckets.total} color="#22c55e" />
      <RatingBar label="良好(60-79)" count={buckets.good} total={buckets.total} color="#3b82f6" />
      <RatingBar label="待改进(<60)" count={buckets.poor} total={buckets.total} color="#f59e0b" />
      <RatingBar label="未评分" count={buckets.unscored} total={buckets.total} color="#9ca3af" />
    </CardShell>
  );
}
