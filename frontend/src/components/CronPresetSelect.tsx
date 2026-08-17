import { Select, Typography } from 'antd';
import { ClockCircleOutlined } from '@ant-design/icons';

const { Text } = Typography;

interface CronPreset {
  label: string;
  value: string;
  category: string;
}

const CRON_PRESETS: CronPreset[] = [
  // 常用
  { label: '每10分钟', value: '0 */10 * * * *', category: '常用' },
  { label: '每30分钟', value: '0 */30 * * * *', category: '常用' },
  { label: '每1小时', value: '0 0 * * * *', category: '常用' },
  { label: '每2小时', value: '0 0 */2 * * *', category: '常用' },
  { label: '每6小时', value: '0 0 */6 * * *', category: '常用' },
  // 定时
  { label: '每天0点', value: '0 0 0 * * *', category: '定时' },
  { label: '每天8:00', value: '0 0 8 * * *', category: '定时' },
  { label: '每天9:00', value: '0 0 9 * * *', category: '定时' },
  { label: '每天12:00', value: '0 0 12 * * *', category: '定时' },
  { label: '每天18:00', value: '0 0 18 * * *', category: '定时' },
  { label: '每天20:00', value: '0 0 20 * * *', category: '定时' },
  // 工作时间
  { label: '工作日8:00', value: '0 0 8 * * 1-5', category: '工作时间' },
  { label: '工作日9:00', value: '0 0 9 * * 1-5', category: '工作时间' },
  { label: '工作日10:00', value: '0 0 10 * * 1-5', category: '工作时间' },
  { label: '工作日14:00', value: '0 0 14 * * 1-5', category: '工作时间' },
  { label: '工作日8-18点每2小时', value: '0 0 8-18/2 * * 1-5', category: '工作时间' },
  { label: '工作日9-18点每小时', value: '0 0 9-18 * * 1-5', category: '工作时间' },
  // 下班时间
  { label: '下班后每30分钟', value: '0 */30 19-23 * * *', category: '下班时间' },
  { label: '22:00-08:00每45分钟', value: '0 */45 22-23,0-8 * * *', category: '下班时间' },
  { label: '22:00-08:00每小时', value: '0 0 22-23,0-8 * * *', category: '下班时间' },
  // 凌晨
  { label: '每天凌晨2点', value: '0 0 2 * * *', category: '凌晨' },
  { label: '每天凌晨3点', value: '0 0 3 * * *', category: '凌晨' },
  { label: '每天凌晨4点', value: '0 0 4 * * *', category: '凌晨' },
];

interface CronPresetSelectProps {
  value: string;
  onChange: (value: string) => void;
  disabled?: boolean;
}

export function CronPresetSelect({ value, onChange, disabled }: CronPresetSelectProps) {
  const categories = [...new Set(CRON_PRESETS.map(p => p.category))];

  return (
    <div style={{ marginBottom: 12 }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 6, marginBottom: 8 }}>
        <ClockCircleOutlined style={{ color: 'var(--color-primary)', fontSize: 12 }} />
        <Text type="secondary" style={{ fontSize: 12 }}>快速选择</Text>
      </div>
      <Select
        value={value}
        onChange={(val: string | undefined) => {
          if (val !== undefined) onChange(val);
        }}
        disabled={disabled}
        style={{ width: '100%' }}
        placeholder="选择常用时间或自定义设置"
        allowClear
        options={categories.map(category => ({
          label: category,
          options: CRON_PRESETS.filter(p => p.category === category).map(p => ({
            label: p.label,
            value: p.value,
          })),
        }))}
      />
    </div>
  );
}
