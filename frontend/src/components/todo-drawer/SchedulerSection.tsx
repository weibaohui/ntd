import { Switch } from 'antd';
import { ClockCircleOutlined } from '@ant-design/icons';
import { Cron } from 'react-js-cron';
import 'react-js-cron/dist/styles.css';
import { DEFAULT_CRON } from './constants';
import { CronPresetSelect } from '../CronPresetSelect';
import { CRON_ZH_LOCALE, cronTo5, cronTo6 } from '../../utils/cron';

export function SchedulerSection({ enabled, config, onEnabledChange, onConfigChange, existingConfig }: {
  enabled: boolean;
  config: string;
  onEnabledChange: (v: boolean) => void;
  onConfigChange: (v: string) => void;
  existingConfig?: string | null;
}) {
  return (
    <div>
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 12 }}>
        <div style={{ fontWeight: 600, fontSize: 14 }}>
          <ClockCircleOutlined style={{ color: 'var(--color-primary)', marginRight: 6 }} />
          定时调度
        </div>
        <Switch
          checked={enabled}
          onChange={(checked) => {
            onEnabledChange(checked);
            if (checked && !config) {
              onConfigChange(DEFAULT_CRON);
            }
          }}
        />
      </div>

      {enabled && (
        <div style={{ marginTop: 12 }}>
          <CronPresetSelect
            value={config || DEFAULT_CRON}
            onChange={(val) => onConfigChange(val)}
          />
          <div style={{ marginTop: 12 }}>
            <Cron
              value={cronTo5(config || DEFAULT_CRON)}
              setValue={(val: string) => onConfigChange(cronTo6(val))}
              locale={CRON_ZH_LOCALE}
              defaultPeriod="hour"
              humanizeLabels
              allowClear={false}
            />
          </div>
        </div>
      )}

      {existingConfig && (
        <div style={{ marginTop: 8, fontSize: 12, color: 'var(--color-text-tertiary)' }}>
          当前配置: <code>{existingConfig}</code>
        </div>
      )}
    </div>
  );
}
