// 群聊响应白名单卡片：添加/删除 Open ID 白名单。

import { Card, AutoComplete, Input, Button, Typography } from 'antd';
const { Paragraph } = Typography;
import type { WhitelistEntry, FeishuSenderItem } from '@/utils/database';

interface WhitelistCardProps {
  whitelist: WhitelistEntry[];
  historySenders: FeishuSenderItem[];
  whitelistOpenId: string;
  whitelistName: string;
  onOpenIdChange: (v: string) => void;
  onNameChange: (v: string) => void;
  onAdd: () => void;
  onDelete: (id: number) => void;
}

export function WhitelistCard({
  whitelist,
  historySenders,
  whitelistOpenId,
  whitelistName,
  onOpenIdChange,
  onNameChange,
  onAdd,
  onDelete,
}: WhitelistCardProps) {
  return (
    <Card title="群聊响应白名单" size="small">
      <Paragraph type="secondary" style={{ fontSize: 13, marginBottom: 12 }}>
        白名单为空时不限制，仅白名单内的用户消息会触发响应
      </Paragraph>

      {/* 添加区域 */}
      <div style={{ display: 'flex', gap: 8, marginBottom: 8 }}>
        <AutoComplete
          size="small"
          placeholder="搜索或粘贴 Open ID"
          style={{ flex: 1 }}
          value={whitelistOpenId}
          onChange={onOpenIdChange}
          options={historySenders.filter(s => s.sender_open_id).map(s => ({
            value: s.sender_open_id,
            label: `${s.sender_nickname || s.sender_open_id} (${s.count}条)`,
          }))}
        />
        <Input
          size="small"
          placeholder="备注名"
          value={whitelistName}
          onChange={e => onNameChange(e.target.value)}
          style={{ width: 100 }}
        />
        <Button size="small" onClick={onAdd}>添加</Button>
      </div>

      {/* 列表 */}
      {whitelist.map(w => (
        <div key={w.id} style={{ display: 'flex', alignItems: 'center', gap: 4, fontSize: 12, marginBottom: 4 }}>
          <span style={{ flex: 1 }}>{w.sender_name || w.sender_open_id}</span>
          <span style={{ color: 'var(--color-text-tertiary)', fontSize: 11 }}>
            {w.sender_open_id.slice(0, 12)}...
          </span>
          <Button
            size="small"
            danger
            type="link"
            style={{ fontSize: 11, padding: 0 }}
            onClick={() => onDelete(w.id)}
          >
            删除
          </Button>
        </div>
      ))}
      {whitelist.length === 0 && (
        <div style={{ fontSize: 12, color: 'var(--color-text-tertiary)' }}>
          暂无白名单，所有用户均可触发响应
        </div>
      )}
    </Card>
  );
}
