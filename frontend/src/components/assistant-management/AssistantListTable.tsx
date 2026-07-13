import { Table, Button, Tag, Space, Popconfirm } from 'antd';
import { SettingOutlined, PoweroffOutlined, DeleteOutlined } from '@ant-design/icons';
import type { AgentBot, ProjectDirectory } from '@/utils/database';

interface AssistantListTableProps {
  bots: AgentBot[];
  workspaces: ProjectDirectory[];
  onOpenConfig: (bot: AgentBot) => void;
  onToggleEnabled: (bot: AgentBot) => void;
  onDelete: (bot: AgentBot) => void;
}

export function AssistantListTable({ bots, workspaces, onOpenConfig, onToggleEnabled, onDelete }: AssistantListTableProps) {
  const getWorkspaceName = (workspaceId: number) => {
    return workspaces.find(w => w.id === workspaceId)?.name || '-';
  };

  const columns = [
    {
      title: '智能助手名称',
      dataIndex: 'bot_name',
      key: 'bot_name',
      width: 180,
      render: (text: string) => <span style={{ fontWeight: 500 }}>{text}</span>,
    },
    {
      title: '类型',
      dataIndex: 'bot_type',
      key: 'bot_type',
      width: 120,
      render: (type: string) => (
        <Tag color={type === 'feishu' ? 'blue' : 'default'}>
          {type === 'feishu' ? '飞书' : type}
        </Tag>
      ),
    },
    {
      title: '当前服务',
      key: 'workspace',
      width: 150,
      render: (_: unknown, record: AgentBot) => (
        <Tag color={record.workspace_id ? 'green' : 'gray'}>
          {record.workspace_id ? getWorkspaceName(record.workspace_id) : '未分配'}
        </Tag>
      ),
    },
    {
      title: 'App ID',
      dataIndex: 'app_id',
      key: 'app_id',
      width: 200,
      render: (text: string, record: AgentBot) => (
        <div>
          <code style={{ fontSize: 12, color: 'var(--color-text-secondary)' }}>{text}</code>
          {record.owner_open_id && (
            <div>
              <code style={{ fontSize: 11, color: 'var(--color-text-tertiary)' }} title="所有者 open_id（推送目标，自动捕获）">{record.owner_open_id}</code>
            </div>
          )}
        </div>
      ),
    },
    {
      title: '状态',
      key: 'enabled',
      width: 100,
      render: (_: unknown, record: AgentBot) => (
        <Tag color={record.enabled ? 'green' : 'red'}>
          {record.enabled ? '运行中' : '已停用'}
        </Tag>
      ),
    },
    {
      title: '创建时间',
      dataIndex: 'created_at',
      key: 'created_at',
      width: 160,
      render: (text: string) => {
        const d = new Date(text);
        return isNaN(d.getTime()) ? text : d.toLocaleString('zh-CN', {
          month: '2-digit',
          day: '2-digit',
          hour: '2-digit',
          minute: '2-digit',
        });
      },
    },
    {
      title: '操作',
      key: 'actions',
      width: 200,
      render: (_: unknown, record: AgentBot) => (
        <Space size="middle">
          <Button type="text" size="small" icon={<SettingOutlined />} onClick={() => onOpenConfig(record)}>
            配置
          </Button>
          <Button
            type="text"
            size="small"
            icon={<PoweroffOutlined />}
            onClick={() => onToggleEnabled(record)}
            danger={record.enabled}
          >
            {record.enabled ? '停用' : '启用'}
          </Button>
          <Popconfirm title="确定删除此智能体？" onConfirm={() => onDelete(record)} okText="删除" cancelText="取消">
            <Button type="text" size="small" icon={<DeleteOutlined />} danger>
              删除
            </Button>
          </Popconfirm>
        </Space>
      ),
    },
  ];

  return (
    <Table
      dataSource={bots}
      columns={columns}
      rowKey="id"
      bordered={false}
      size="small"
      pagination={{
        pageSize: 10,
        showSizeChanger: true,
        showQuickJumper: true,
        showTotal: (total) => `共 ${total} 个智能助手`,
      }}
    />
  );
}