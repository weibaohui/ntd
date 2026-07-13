// 历史消息拉取群卡片：填写需要定期拉取历史消息的群 chat_id，增删管理。
// 受控组件：state/handler 由父组件持有，本组件只负责展示与事件回调。
// 被智能体配置抽屉（AssistantConfigDrawer）与详情页（AssistantDetailPage）共用。

import { Card, Input, Button, Popconfirm } from 'antd';
import type { FeishuHistoryChat } from '@/types';

interface HistoryChatsCardProps {
  /** 当前 bot 已配置的拉取群列表 */
  chats: FeishuHistoryChat[];
  /** 新增表单：群 chat_id 输入值 */
  chatId: string;
  /** 新增表单：群名称备注输入值 */
  chatName: string;
  onChatIdChange: (v: string) => void;
  onChatNameChange: (v: string) => void;
  onAdd: () => void;
  onDelete: (id: number) => void;
}

export function HistoryChatsCard({
  chats,
  chatId,
  chatName,
  onChatIdChange,
  onChatNameChange,
  onAdd,
  onDelete,
}: HistoryChatsCardProps) {
  return (
    <Card title="历史消息拉取群" size="small" style={{ marginBottom: 16 }}>
      <div style={{ fontSize: 13, color: 'var(--color-text-secondary)', marginBottom: 12 }}>
        填写需要定期拉取历史消息的群 chat_id（形如 oc_xxxxxxxx）
      </div>

      {/* 添加区域：chat_id 必填、备注可选 */}
      <div style={{ display: 'flex', gap: 8, marginBottom: 8 }}>
        <Input
          size="small"
          placeholder="群 chat_id（oc_xxxxxxxx）"
          style={{ flex: 1 }}
          value={chatId}
          onChange={e => onChatIdChange(e.target.value)}
        />
        <Input
          size="small"
          placeholder="群名称备注"
          style={{ width: 120 }}
          value={chatName}
          onChange={e => onChatNameChange(e.target.value)}
        />
        <Button size="small" onClick={onAdd}>添加</Button>
      </div>

      {/* 已配置列表 */}
      {chats.map(c => (
        <div key={c.id} style={{ display: 'flex', alignItems: 'center', gap: 4, fontSize: 12, marginBottom: 4 }}>
          <span style={{ flex: 1 }}>{c.chat_name || c.chat_id}</span>
          <span style={{ color: 'var(--color-text-tertiary)', fontSize: 11 }}>{c.chat_id.slice(0, 12)}...</span>
          <Popconfirm title="确定删除该拉取群？" onConfirm={() => onDelete(c.id)}>
            <Button size="small" danger type="link" style={{ fontSize: 11, padding: 0 }}>
              删除
            </Button>
          </Popconfirm>
        </div>
      ))}
      {chats.length === 0 && (
        <div style={{ fontSize: 12, color: 'var(--color-text-tertiary)' }}>暂无拉取群</div>
      )}
    </Card>
  );
}
