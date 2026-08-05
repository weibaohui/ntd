// 讨论区输入器：Markdown 编辑 + @执行器 快选 + 发送。
// MVP：@执行器 走下拉快选（插入 @<规范名>），@专家 由用户手敲 @专家名（完整 @popover 属 M4）。
// 后端按正文里的 @token 解析触发，这里只负责便捷插入。

import { Button, Space, Select, Typography } from 'antd';
import { SendOutlined } from '@ant-design/icons';
import { MdEditor } from '@/components/MdEditor';
import { EXECUTORS_FOR_PICKER } from '@/utils/executors';

const { Text } = Typography;

interface DiscussionComposerProps {
  value: string;
  onChange: (v: string) => void;
  onSend: () => void;
  sending: boolean;
  /** 回复目标提示（如 "#3 张三"）；null=主楼层新帖。 */
  replyTo?: string | null;
  onCancelReply?: () => void;
}

/**
 * 把一段 @token 追加到正文末尾，保证前面有空格分隔（避免粘连成 `xxx@codex`）。
 * 纯函数便于理解与测试。
 */
function appendMention(value: string, name: string): string {
  const needSpace = value.length > 0 && !value.endsWith(' ');
  return `${value}${needSpace ? ' ' : ''}@${name} `;
}

export function DiscussionComposer({
  value, onChange, onSend, sending, replyTo, onCancelReply,
}: DiscussionComposerProps) {
  // Select 用 null 受控值：每次选择都触发 onChange，同一执行器可重复插入。
  const insertExecutor = (name: string) => {
    onChange(appendMention(value, name));
  };
  const canSend = value.trim().length > 0 && !sending;

  return (
    <div>
      {replyTo ? (
        <div style={{ marginBottom: 8 }}>
          <Text type="secondary">回复 {replyTo} </Text>
          <Button size="small" type="link" onClick={onCancelReply}>取消</Button>
        </div>
      ) : null}
      <MdEditor value={value} onChange={onChange} height={140} />
      <Space style={{ marginTop: 8, width: '100%', justifyContent: 'space-between', flexWrap: 'wrap' }}>
        <Space size={12} wrap>
          <Select
            size="small"
            placeholder="@ 执行器"
            value={null}
            style={{ minWidth: 150 }}
            options={EXECUTORS_FOR_PICKER.map((e) => ({ value: e.value, label: e.label }))}
            onChange={insertExecutor}
            showSearch
            optionFilterProp="label"
          />
          <Text type="secondary" style={{ fontSize: 12 }}>
            @执行器 或 @专家名 触发智能体干活（如 @codex、@前端架构师）
          </Text>
        </Space>
        <Button type="primary" icon={<SendOutlined />} disabled={!canSend} loading={sending} onClick={onSend}>
          发送
        </Button>
      </Space>
    </div>
  );
}
