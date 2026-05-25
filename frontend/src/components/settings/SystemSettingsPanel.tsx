import { Form, Input, InputNumber, Select, Button, Spin } from 'antd';

const LOG_LEVELS = ['DEBUG', 'INFO', 'WARN', 'ERROR'];

const TIMEZONES = [
  { value: 'UTC', label: 'UTC (世界协调时间)' },
  { value: 'Asia/Shanghai', label: 'Asia/Shanghai (北京时间, UTC+8)' },
  { value: 'Asia/Tokyo', label: 'Asia/Tokyo (东京时间, UTC+9)' },
  { value: 'Asia/Seoul', label: 'Asia/Seoul (首尔时间, UTC+9)' },
  { value: 'Asia/Singapore', label: 'Asia/Singapore (新加坡时间, UTC+8)' },
  { value: 'Asia/Hong_Kong', label: 'Asia/Hong_Kong (香港时间, UTC+8)' },
  { value: 'Asia/Dubai', label: 'Asia/Dubai (迪拜时间, UTC+4)' },
  { value: 'Europe/London', label: 'Europe/London (伦敦时间, UTC+0)' },
  { value: 'Europe/Paris', label: 'Europe/Paris (巴黎时间, UTC+1)' },
  { value: 'Europe/Berlin', label: 'Europe/Berlin (柏林时间, UTC+1)' },
  { value: 'America/New_York', label: 'America/New_York (纽约时间, UTC-5)' },
  { value: 'America/Los_Angeles', label: 'America/Los_Angeles (洛杉矶时间, UTC-8)' },
  { value: 'America/Chicago', label: 'America/Chicago (芝加哥时间, UTC-6)' },
  { value: 'America/Sao_Paulo', label: 'America/Sao_Paulo (圣保罗时间, UTC-3)' },
  { value: 'Australia/Sydney', label: 'Australia/Sydney (悉尼时间, UTC+10)' },
];

const { Option } = Select;

export function SystemSettingsPanel({ configForm, configSaving, configLoading, handleSaveConfig }: {
  configForm: any;
  configSaving: boolean;
  configLoading: boolean;
  handleSaveConfig: () => Promise<void>;
}) {
  return (
    <Spin spinning={configLoading}>
      <Form
        form={configForm}
        layout="vertical"
        style={{ maxWidth: 600 }}
        initialValues={{
          port: 8088,
          host: '0.0.0.0',
          db_path: '~/.ntd/data.db',
          log_level: 'INFO',
        }}
      >
        <Form.Item
          name="port"
          label="服务端口"
          rules={[{ required: true, type: 'integer', min: 1, max: 65535 }]}
        >
          <InputNumber style={{ width: '100%' }} placeholder="8088" />
        </Form.Item>
        <Form.Item
          name="host"
          label="服务地址"
          rules={[{ required: true }]}
        >
          <Input placeholder="0.0.0.0" />
        </Form.Item>
        <Form.Item
          name="db_path"
          label="数据库路径"
          rules={[{ required: true }]}
        >
          <Input placeholder="~/.ntd/data.db" />
        </Form.Item>
        <Form.Item
          name="log_level"
          label="日志级别"
          rules={[{ required: true }]}
        >
          <Select placeholder="选择日志级别">
            {LOG_LEVELS.map((level) => (
              <Option key={level} value={level}>
                {level}
              </Option>
            ))}
          </Select>
        </Form.Item>
        <Form.Item
          name="scheduler_default_timezone"
          label="定时任务默认时区"
          tooltip="设置创建定时任务时的默认时区。例如：选择 Asia/Shanghai 后，每天 9:00 执行的任务会按北京时间 9:00 执行（而不是服务器本地时间）。"
        >
          <Select
            showSearch
            placeholder="选择默认时区"
            allowClear
            filterOption={(input, option) =>
              (option?.label ?? '').toLowerCase().includes(input.toLowerCase())
            }
            options={TIMEZONES}
          />
        </Form.Item>
        <Form.Item>
          <Button
            type="primary"
            onClick={handleSaveConfig}
            loading={configSaving}
            disabled={configLoading}
          >
            保存配置
          </Button>
        </Form.Item>
      </Form>
    </Spin>
  );
}
