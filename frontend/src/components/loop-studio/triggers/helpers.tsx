// 触发器元数据常量：图标、标签、描述的静态定义。
// 7 种触发类型全部以行内 toggle 展示，每种类型有独立的配置弹窗。

import {
  ClockCircleOutlined,
  PlayCircleOutlined,
  MessageOutlined,
  ThunderboltOutlined,
  CheckCircleOutlined,
  SyncOutlined,
  TagOutlined,
} from '@ant-design/icons';

export const TRIGGER_META: Record<string, {
  icon: React.ReactNode;
  label: string;
  desc: string;
  // 不需要配置（manual 等）
  noConfig?: boolean;
}> = {
  manual: {
    icon: <PlayCircleOutlined />,
    label: '手动触发',
    desc: '通过「触发」按钮主动启动',
    noConfig: true,
  },
  cron: {
    icon: <ClockCircleOutlined />,
    label: '定时调度',
    desc: '按 cron 表达式周期性触发',
  },
  feishu_message: {
    icon: <MessageOutlined />,
    label: '飞书消息',
    desc: '收到飞书消息时触发',
  },
  feishu_command: {
    icon: <ThunderboltOutlined />,
    label: '飞书指令',
    desc: '飞书内指定指令触发',
  },
  todo_completed: {
    icon: <CheckCircleOutlined />,
    label: 'Todo 完成',
    desc: '某个 todo 完成时触发',
  },
  todo_state_changed: {
    icon: <SyncOutlined />,
    label: 'Todo 状态变更',
    desc: '某个 todo 状态变化时触发',
  },
  tag_added: {
    icon: <TagOutlined />,
    label: '标签添加',
    desc: '某个标签被添加到 todo 时触发',
  },
};
