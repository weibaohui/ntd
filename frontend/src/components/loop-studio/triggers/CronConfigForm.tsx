// 定时调度配置表单：复用 todo 定时调度的 CronPresetSelect + react-js-cron。
// 存储格式（保持与后端一致）：{"cron":"0 0 9 * * *","timezone":"Asia/Shanghai"}

import { useState, useCallback, useEffect, useMemo } from 'react';
import { Form, Select } from 'antd';
import { CronPresetSelect } from '@/components/CronPresetSelect';
import { Cron } from 'react-js-cron';
import 'react-js-cron/dist/styles.css';
import { CRON_ZH_LOCALE, cronTo5, cronTo6 } from '@/utils/cron';

interface CronConfigFormProps {
  value: string;
  onChange: (json: string) => void;
}

export function CronConfigForm({ value, onChange }: CronConfigFormProps) {
  // 解析当前值，提取 cron 和 timezone
  const parsed = useMemo(() => {
    try {
      const v = JSON.parse(value || '{}');
      return {
        cron: v.cron || '0 0 9 * * *',
        timezone: v.timezone || 'Asia/Shanghai',
      };
    } catch {
      return { cron: '0 0 9 * * *', timezone: 'Asia/Shanghai' };
    }
  }, [value]);

  const [cronExpr, setCronExpr] = useState(parsed.cron);
  const [timezone, setTimezone] = useState(parsed.timezone);

  // 同步回外层
  const sync = useCallback((c: string, tz: string) => {
    onChange(JSON.stringify({ cron: c, timezone: tz }));
  }, [onChange]);

  useEffect(() => { sync(cronExpr, timezone); }, [cronExpr, timezone, sync]);

  return (
    <div>
      {/* 快速预设选择 */}
      <CronPresetSelect
        value={cronExpr}
        onChange={(val) => setCronExpr(val)}
      />
      {/* react-js-cron 图形化编辑器 */}
      <div style={{ marginTop: 8, marginBottom: 12 }}>
        <Cron
          value={cronTo5(cronExpr)}
          setValue={(val: string) => setCronExpr(cronTo6(val))}
          locale={CRON_ZH_LOCALE}
          defaultPeriod="hour"
          humanizeLabels
          allowClear={false}
        />
      </div>
      {/* 时区选择 */}
      <Form.Item label="时区" tooltip="cron 表达式在该时区的本地时间执行">
        <Select
          value={timezone}
          onChange={(v) => setTimezone(v)}
          showSearch
          options={[
            { value: 'Asia/Shanghai', label: 'Asia/Shanghai (UTC+8)' },
            { value: 'Asia/Tokyo', label: 'Asia/Tokyo (UTC+9)' },
            { value: 'America/New_York', label: 'America/New_York (UTC-5/-4)' },
            { value: 'America/Los_Angeles', label: 'America/Los_Angeles (UTC-8/-7)' },
            { value: 'Europe/London', label: 'Europe/London (UTC+0/+1)' },
            { value: 'Europe/Berlin', label: 'Europe/Berlin (UTC+1/+2)' },
            { value: 'UTC', label: 'UTC' },
          ]}
        />
      </Form.Item>
    </div>
  );
}
