import { Button, Select, Table, Tag, Typography, Modal, Form, Input, Space, Tooltip, message } from 'antd';
import { ReloadOutlined, PlusOutlined, HistoryOutlined, QuestionCircleOutlined, CopyOutlined } from '@ant-design/icons';
import * as db from '../../../utils/database';
import type { FeishuHistoryMessage, FeishuHistoryChat } from '../../../types';

const { Option } = Select;

export function RecordTab({
  historyMessages, historyChats, historySenders, historyLoading,
  historyTotal, historyPage, historyPageSize,
  historySelectedChatId, historyIsHistory, historySelectedSenderId,
  historyAddModalOpen, historyForm,
  agentBots,
  onViewMsg, onViewTodo, onViewExecutionRecord,
  onRefreshMessages,
  onChatFilterChange, onSenderFilterChange, onHistoryFilterChange,
  onPageChange,
  onAddClick, onAddChat, onAddModalCancel,
}: {
  historyMessages: FeishuHistoryMessage[];
  historyChats: FeishuHistoryChat[];
  historySenders: db.FeishuSenderItem[];
  historyLoading: boolean;
  historyTotal: number;
  historyPage: number;
  historyPageSize: number;
  historySelectedChatId: string | undefined;
  historyIsHistory: boolean | undefined;
  historySelectedSenderId: string | undefined;
  historyAddModalOpen: boolean;
  historyForm: any;
  agentBots: db.AgentBot[];
  onViewMsg: (msg: string) => void;
  onViewTodo: (todoId: number) => void;
  onViewExecutionRecord: (recordId: number) => Promise<void>;
  onRefreshMessages: () => void;
  onChatFilterChange: (v: string | undefined) => void;
  onSenderFilterChange: (v: string | undefined) => void;
  onHistoryFilterChange: (v: boolean | undefined) => void;
  onPageChange: (page: number, pageSize: number) => void;
  onAddClick: () => void;
  onAddChat: () => Promise<void>;
  onAddModalCancel: () => void;
}) {
  return (
    <div className="settings-history-tab">
      <div
        style={{
          marginBottom: 16,
          display: 'flex',
          flexWrap: 'wrap',
          gap: 8,
          justifyContent: 'space-between',
          alignItems: 'center',
        }}
      >
        <Space>
          <HistoryOutlined />
          <span style={{ fontWeight: 600 }}>飞书群聊消息</span>
          <Typography.Text type="secondary" style={{ fontSize: 12 }}>
            <Tooltip title={
              <div style={{ fontSize: 12, lineHeight: 1.6 }}>
                <b>实时消息：</b>通过 WebSocket 实时接收的消息，接收后立即处理并触发相关事件<br/>
                <b>历史消息：</b>通过轮询 API 拉取的群聊历史记录，不会触发实时处理事件
              </div>
            }>
              <span style={{ cursor: 'help' }}><QuestionCircleOutlined /></span>
            </Tooltip>
          </Typography.Text>
        </Space>
        <Space wrap>
          <Select
            placeholder="筛选群聊"
            allowClear
            style={{ width: 200 }}
            value={historySelectedChatId}
            onChange={onChatFilterChange}
            onClear={() => onChatFilterChange(undefined)}
          >
            {historyChats.map((chat) => (
              <Option key={chat.chat_id} value={chat.chat_id}>
                {chat.chat_name || chat.chat_id}
              </Option>
            ))}
          </Select>
          <Select
            placeholder="筛选发送者"
            allowClear
            style={{ width: 150 }}
            value={historySelectedSenderId}
            onChange={onSenderFilterChange}
            onClear={() => onSenderFilterChange(undefined)}
          >
            {historySenders.map((item) => (
              <Option key={item.sender_open_id} value={item.sender_open_id}>
                {item.sender_nickname || item.sender_open_id.slice(0, 12)} ({item.count}条)
              </Option>
            ))}
          </Select>
          <Select
            placeholder="消息来源"
            style={{ width: 130 }}
            value={historyIsHistory}
            onChange={onHistoryFilterChange}
            allowClear
          >
            <Option value={true}>仅历史消息</Option>
            <Option value={false}>仅实时消息</Option>
          </Select>
          <Button icon={<ReloadOutlined />} onClick={onRefreshMessages} size="middle">
            刷新
          </Button>
          <Button type="primary" icon={<PlusOutlined />} onClick={onAddClick} size="middle">
            添加
          </Button>
        </Space>
      </div>

      <Table
        dataSource={historyMessages}
        rowKey="id"
        loading={historyLoading}
        scroll={{ x: 'max-content' }}
        pagination={{
          current: historyPage,
          pageSize: historyPageSize,
          total: historyTotal,
          showSizeChanger: true,
          showQuickJumper: true,
          showTotal: (t: number) => `共 ${t} 条`,
          onChange: onPageChange,
        }}
        size="middle"
        columns={[
          {
            title: '时间',
            dataIndex: 'created_at',
            key: 'created_at',
            width: 150,
            render: (text: string) => {
              if (!text) return '-';
              const d = new Date(text);
              return isNaN(d.getTime()) ? text : d.toLocaleString('zh-CN');
            },
          },
          {
            title: '来源',
            key: 'source',
            width: 90,
            render: (_, record) => (
              <Tag color={record.is_history ? 'orange' : 'cyan'}>
                {record.is_history ? '历史' : '实时'}
              </Tag>
            ),
          },
          {
            title: '发送者',
            key: 'sender',
            width: 160,
            render: (_, record) => {
              const isBot = record.sender_type === 'app';
              return (
                <Space size={2}>
                  <Tag color={isBot ? 'blue' : 'green'}>
                    {isBot ? '智能体' : '用户'}
                  </Tag>
                  <Typography.Text type="secondary" style={{ fontSize: 12 }}>
                    {record.sender_nickname || record.sender_open_id?.slice(0, 8) || '-'}
                  </Typography.Text>
                  {record.sender_open_id && (
                    <Button
                      size="small"
                      type="link"
                      icon={<CopyOutlined />}
                      style={{ fontSize: 10, padding: 0 }}
                      onClick={() => {
                        navigator.clipboard.writeText(record.sender_open_id);
                        message.success('已复制 Open ID');
                      }}
                    />
                  )}
                </Space>
              );
            },
          },
          {
            title: '内容',
            dataIndex: 'content',
            key: 'content',
            width: 200,
            render: (content: string, record) => {
              let text: string;
              if (record.msg_type === 'text') {
                try {
                  const parsed = JSON.parse(content);
                  text = parsed.text || content;
                } catch {
                  text = content;
                }
              } else {
                return <Tag>{record.msg_type}</Tag>;
              }
              const MAX = 40;
              const truncated = text.length > MAX ? text.slice(0, MAX) + '...' : text;
              return (
                <span
                  style={{ cursor: 'pointer', fontSize: 12 }}
                  onClick={() => onViewMsg(text)}
                >
                  {truncated}
                </span>
              );
            },
          },
          {
            title: '处理状态',
            key: 'processed',
            width: 90,
            render: (_, record) => (
              record.processed ? (
                <Tag color="green">已处理</Tag>
              ) : (
                <Tag color="default">未处理</Tag>
              )
            ),
          },
          {
            title: '触发Todo',
            key: 'processed_todo_id',
            width: 80,
            render: (_, record) => (
              record.processed_todo_id ? (
                <Typography.Link
                  style={{ fontSize: 12 }}
                  onClick={() => onViewTodo(record.processed_todo_id!)}
                >
                  #{record.processed_todo_id}
                </Typography.Link>
              ) : (
                <span style={{ color: 'var(--color-text-tertiary)', fontSize: 12 }}>-</span>
              )
            ),
          },
          {
            title: '执行记录',
            key: 'execution_record_id',
            width: 80,
            render: (_, record) => (
              record.execution_record_id ? (
                <Typography.Link
                  style={{ fontSize: 12 }}
                  onClick={() => onViewExecutionRecord(record.execution_record_id!)}
                >
                  #{record.execution_record_id}
                </Typography.Link>
              ) : (
                <span style={{ color: 'var(--color-text-tertiary)', fontSize: 12 }}>-</span>
              )
            ),
          },
        ]}
      />

      <Modal
        title="添加监听群聊"
        open={historyAddModalOpen}
        onOk={onAddChat}
        onCancel={onAddModalCancel}
        width={520}
      >
        <Form form={historyForm} layout="vertical">
          <Form.Item
            name="bot_id"
            label="机器人"
            rules={[{ required: true, message: '请选择机器人' }]}
          >
            <Select placeholder="请选择机器人">
              {agentBots.filter(b => b.bot_type === 'feishu').map((bot) => (
                <Option key={bot.id} value={bot.id}>
                  {bot.bot_name}
                </Option>
              ))}
            </Select>
          </Form.Item>
          <Form.Item
            name="chat_id"
            label="群聊 ID"
            rules={[{ required: true, message: '请输入群聊 ID' }]}
          >
            <Input placeholder="请输入飞书群聊 ID" />
          </Form.Item>
          <Form.Item name="chat_name" label="群聊名称（可选）">
            <Input placeholder="请输入群聊名称，方便识别" />
          </Form.Item>
        </Form>
      </Modal>
    </div>
  );
}
