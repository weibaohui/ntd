// 配置弹窗内容分发组件：根据 trigger_type 返回对应的配置表单组件。
// 对于 manual 这种无需配置的类型，返回 null。

import { Form, Input } from 'antd';
import { CronConfigForm } from './CronConfigForm';
import { TodoSelectorForm } from './TodoSelectorForm';
import { TagSelectorForm } from './TagSelectorForm';
import { FeishuMessageConfigForm } from './FeishuMessageConfigForm';
import { FeishuCommandConfigForm } from './FeishuCommandConfigForm';
import { TodoStateChangedConfigForm } from './TodoStateChangedConfigForm';

interface TriggerConfigContentProps {
  type: string;
  value: string;
  onChange: (json: string) => void;
  workspaceId?: number | null;
}

export function TriggerConfigContent({ type, value, onChange, workspaceId }: TriggerConfigContentProps) {
  switch (type) {
    case 'cron':
      return <CronConfigForm value={value} onChange={onChange} />;
    case 'feishu_message':
      return <FeishuMessageConfigForm value={value} onChange={onChange} />;
    case 'feishu_command':
      return <FeishuCommandConfigForm value={value} onChange={onChange} />;
    case 'todo_completed':
      return <TodoSelectorForm value={value} onChange={onChange} workspaceId={workspaceId} />;
    case 'todo_state_changed':
      return <TodoStateChangedConfigForm value={value} onChange={onChange} workspaceId={workspaceId} />;
    case 'tag_added':
      return <TagSelectorForm value={value} onChange={onChange} />;
    default:
      // 兜底：未知类型，展示原始 JSON 编辑区
      return (
        <Form.Item label="配置 (JSON)" name="config">
          <Input.TextArea
            rows={5}
            placeholder="{}"
            style={{ fontFamily: 'monospace', fontSize: 12 }}
          />
        </Form.Item>
      );
  }
}
