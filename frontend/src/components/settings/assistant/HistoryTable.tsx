// 历史消息记录表格：支持按群聊/消息类型筛选，分页展示。

import { Card, Select, Button, Table, Tag, Typography } from 'antd';
import { ReloadOutlined } from '@ant-design/icons';
import type { ColumnsType } from 'antd/es/table';
import type { FeishuHistoryMessage, FeishuHistoryChat } from '@/types';

interface HistoryTableProps {
  messages: FeishuHistoryMessage[];
  chats: FeishuHistoryChat[];
  loading: boolean;
  page: number;
  pageSize: number;
  total: number;
  selectedChatId: string | undefined;
  isHistory: boolean | undefined;
  onChatChange: (v: string | undefined) => void;
  onHistoryChange: (v: boolean | undefined) => void;
  onRefresh: () => void;
  onPageChange: (page: number, pageSize: number) => void;
  onViewExecutionRecord: (recordId: number) => void;
  onProcessedTypeClick: (record: FeishuHistoryMessage) => void;
}

// processed_type 英文 → 中文映射
const processedTypeLabel = (type: string | null): string => {
  const map: Record<string, string> = {
    'default_response': '默认响应-事项',
    'default_response_executor': '默认响应-执行器',
    'default_response_loop': '默认响应-环路',
    'slash_command': '斜杠命令-事项',
    'slash_command_loop': '斜杠命令-环路',
    'feishu_project_bind': '项目绑定-事项',
  };
  return map[type || ''] || type || '-';
};

// 判断是否为环路类型
const isLoopType = (type: string | null): boolean => type === 'slash_command_loop';

export function HistoryTable({
  messages,
  chats,
  loading,
  page,
  pageSize,
  total,
  selectedChatId,
  isHistory,
  onChatChange,
  onHistoryChange,
  onRefresh,
  onPageChange,
  onViewExecutionRecord,
  onProcessedTypeClick,
}: HistoryTableProps) {
  const columns: ColumnsType<FeishuHistoryMessage> = [
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
      width: 80,
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
          <span style={{ fontSize: 12 }}>
            <Tag color={isBot ? 'blue' : 'green'}>
              {isBot ? '智能体' : '用户'}
            </Tag>
            {record.sender_nickname || record.sender_open_id?.slice(0, 8) || '-'}
          </span>
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
            text = content || '';
          }
        } else {
          return <Tag>{record.msg_type}</Tag>;
        }
        const MAX = 40;
        const truncated = text.length > MAX ? text.slice(0, MAX) + '...' : text;
        return (
          <span style={{ cursor: 'pointer', fontSize: 12 }}>
            {truncated}
          </span>
        );
      },
    },
    {
      title: '处理状态',
      key: 'processed',
      width: 120,
      render: (_, record) => (
        record.processed
          ? record.error
            ? <Tag color="volcano">已处理({record.error})</Tag>
            : <Tag color="green">已处理</Tag>
          : <Tag color="default">未处理</Tag>
      ),
    },
    {
      title: '执行记录',
      key: 'execution_record_id',
      width: 80,
      render: (_, record) => {
        if (isLoopType(record.processed_type) && record.execution_record_id) {
          return (
            <Typography.Link style={{ fontSize: 12 }} onClick={() => onProcessedTypeClick(record)}>
              #{record.execution_record_id}
            </Typography.Link>
          );
        }
        if (record.execution_record_id) {
          return (
            <Typography.Link style={{ fontSize: 12 }} onClick={() => onViewExecutionRecord(record.execution_record_id!)}>
              #{record.execution_record_id}
            </Typography.Link>
          );
        }
        return <span style={{ color: 'var(--color-text-tertiary)', fontSize: 12 }}>-</span>;
      },
    },
    {
      title: '工作空间',
      key: 'workspace_id',
      width: 80,
      render: (_, record) => (
        record.workspace_id
          ? <Typography.Text style={{ fontSize: 12 }}>#{record.workspace_id}</Typography.Text>
          : <span style={{ color: 'var(--color-text-tertiary)', fontSize: 12 }}>-</span>
      ),
    },
    {
      title: '处理类型',
      key: 'processed_type',
      width: 110,
      render: (_, record) => (
        <span style={{ fontSize: 12 }}>{processedTypeLabel(record.processed_type)}</span>
      ),
    },
    {
      title: '处理ID',
      key: 'processed_id',
      width: 80,
      render: (_, record) => (
        record.processed_id
          ? <Typography.Text style={{ fontSize: 12 }}>#{record.processed_id}</Typography.Text>
          : <span style={{ color: 'var(--color-text-tertiary)', fontSize: 12 }}>-</span>
      ),
    },
  ];

  return (
    <Card title="历史消息" size="small" style={{ marginBottom: 16 }}>
      {/* 筛选栏 */}
      <div style={{ display: 'flex', gap: 12, marginBottom: 12, flexWrap: 'wrap' }}>
        <Select
          size="small"
          placeholder="筛选群聊"
          allowClear
          style={{ width: 150 }}
          value={selectedChatId}
          onChange={v => { onChatChange(v); onPageChange(1, pageSize); }}
          options={chats.map(c => ({ value: c.chat_id, label: c.chat_name || c.chat_id }))}
        />
        <Select
          size="small"
          placeholder="消息类型"
          allowClear
          style={{ width: 120 }}
          value={isHistory}
          onChange={v => { onHistoryChange(v); onPageChange(1, pageSize); }}
          options={[{ value: true, label: '历史消息' }, { value: false, label: '实时消息' }]}
        />
        <Button size="small" icon={<ReloadOutlined />} onClick={onRefresh}>刷新</Button>
      </div>

      {/* 表格 */}
      <Table
        dataSource={messages}
        rowKey="id"
        loading={loading}
        size="small"
        scroll={{ x: 'max-content' }}
        pagination={{
          current: page,
          pageSize,
          total,
          showSizeChanger: true,
          showQuickJumper: true,
          showTotal: (t: number) => `共 ${t} 条`,
          onChange: onPageChange,
        }}
        columns={columns}
      />
    </Card>
  );
}
