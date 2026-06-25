import { useState, useEffect } from 'react';
import { Table, Button, Space, Switch, Popconfirm, message, Modal, Select, Tooltip } from 'antd';
import { DeleteOutlined, SwapOutlined } from '@ant-design/icons';
import type { ColumnsType } from 'antd/es/table';
import * as db from '@/utils/database';
import type { AgentBot, ProjectDirectory } from '@/utils/database';

interface WorkspaceAgentPanelProps {
  workspaceId: number;
  onBotChanged?: () => void;
}

export function WorkspaceAgentPanel({ workspaceId, onBotChanged }: WorkspaceAgentPanelProps) {
  const [bots, setBots] = useState<AgentBot[]>([]);
  const [allBots, setAllBots] = useState<AgentBot[]>([]);
  const [workspaces, setWorkspaces] = useState<ProjectDirectory[]>([]);
  const [loading, setLoading] = useState(false);
  const [moveModalVisible, setMoveModalVisible] = useState(false);
  const [movingBotId, setMovingBotId] = useState<number | null>(null);
  const [targetWorkspaceId, setTargetWorkspaceId] = useState<number | null>(null);

  const loadBots = () => {
    setLoading(true);
    Promise.all([
      db.getAgentBots(),
      db.getProjectDirectories(),
    ])
      .then(([botsData, dirsData]) => {
        setAllBots(botsData);
        // 筛选当前 workspace 的 bots
        setBots(botsData.filter(b => b.workspace_id === workspaceId));
        setWorkspaces(dirsData);
      })
      .catch((err: any) => message.error('加载智能体失败: ' + (err?.message || String(err))))
      .finally(() => setLoading(false));
  };

  useEffect(() => {
    loadBots();
  }, [workspaceId]);

  const handleDelete = async (botId: number) => {
    try {
      await db.deleteAgentBot(botId);
      setBots(prev => prev.filter(b => b.id !== botId));
      message.success('删除成功');
      onBotChanged?.();
    } catch (err: any) {
      message.error('删除失败: ' + (err?.message || String(err)));
    }
  };

  const openMoveModal = (botId: number) => {
    setMovingBotId(botId);
    setTargetWorkspaceId(null);
    setMoveModalVisible(true);
  };

  const handleMove = async () => {
    if (!movingBotId || !targetWorkspaceId) {
      message.error('请选择目标工作空间');
      return;
    }
    try {
      await db.moveBotToWorkspace(movingBotId, targetWorkspaceId);
      message.success('已移动到新工作空间，原有绑定已失效');
      setMoveModalVisible(false);
      loadBots();
      onBotChanged?.();
    } catch (err: any) {
      message.error('移动失败: ' + (err?.message || String(err)));
    }
  };

  const getWorkspaceName = (wsId: number) => {
    const ws = workspaces.find(w => w.id === wsId);
    return ws ? ws.name : `工作空间 #${wsId}`;
  };

  const columns: ColumnsType<AgentBot> = [
    {
      title: '名称',
      dataIndex: 'bot_name',
      key: 'bot_name',
    },
    {
      title: '类型',
      dataIndex: 'bot_type',
      key: 'bot_type',
    },
    {
      title: 'App ID',
      dataIndex: 'app_id',
      key: 'app_id',
      render: (appId: string) => (
        <Tooltip title={appId}>{appId.slice(0, 12)}...</Tooltip>
      ),
    },
    {
      title: '启用',
      dataIndex: 'enabled',
      key: 'enabled',
      width: 80,
      render: (enabled: boolean) => <Switch checked={enabled} disabled />,
    },
    {
      title: '创建时间',
      dataIndex: 'created_at',
      key: 'created_at',
      render: (t: string) => new Date(t).toLocaleString(),
    },
    {
      title: '操作',
      key: 'actions',
      width: 150,
      render: (_: any, bot: AgentBot) => (
        <Space>
          <Button
            type="text"
            size="small"
            icon={<SwapOutlined />}
            onClick={() => openMoveModal(bot.id)}
            title="变更工作空间"
          />
          <Popconfirm
            title="确定删除此智能体？相关绑定记录也将被清除"
            onConfirm={() => handleDelete(bot.id)}
            okText="删除"
            cancelText="取消"
          >
            <Button type="text" size="small" danger icon={<DeleteOutlined />} />
          </Popconfirm>
        </Space>
      ),
    },
  ];

  // 其他工作空间的 bots
  const otherBots = allBots.filter(b => b.workspace_id !== workspaceId);

  return (
    <div>
      <div style={{ marginBottom: 16 }}>
        <span style={{ color: '#666' }}>
          当前工作空间共有 {bots.length} 个智能体
        </span>
      </div>

      <Table
        columns={columns}
        dataSource={bots}
        rowKey="id"
        loading={loading}
        pagination={false}
        locale={{ emptyText: '暂无智能体' }}
      />

      {otherBots.length > 0 && (
        <>
          <div style={{ marginTop: 24, marginBottom: 8, color: '#666' }}>
            其他工作空间的智能体（可移动到当前工作空间）
          </div>
          <Table
            columns={[
              { title: '名称', dataIndex: 'bot_name', key: 'bot_name' },
              { title: '当前工作空间', dataIndex: 'workspace_id', key: 'workspace_id', render: (id: number) => getWorkspaceName(id) },
              {
                title: '操作',
                key: 'actions',
                render: (_: any, bot: AgentBot) => (
                  <Button size="small" icon={<SwapOutlined />} onClick={() => openMoveModal(bot.id)}>
                    移动到当前
                  </Button>
                ),
              },
            ]}
            dataSource={otherBots}
            rowKey="id"
            pagination={false}
            size="small"
          />
        </>
      )}

      <Modal
        title="变更智能体工作空间"
        open={moveModalVisible}
        onOk={handleMove}
        onCancel={() => setMoveModalVisible(false)}
        okText="确认移动"
        cancelText="取消"
      >
        <div style={{ marginBottom: 16, padding: '12px', background: '#fffbe6', borderRadius: 4 }}>
          ⚠️ 移动后，该智能体在原有工作空间的所有聊天绑定将全部失效，需要重新绑定
        </div>
        <div style={{ marginBottom: 16 }}>
          <label style={{ display: 'block', marginBottom: 8 }}>目标工作空间</label>
          <Select
            style={{ width: '100%' }}
            placeholder="选择目标工作空间"
            value={targetWorkspaceId}
            onChange={setTargetWorkspaceId}
          >
            {workspaces
              .filter(w => w.id !== workspaceId)
              .map(ws => (
                <Select.Option key={ws.id} value={ws.id}>
                  {ws.name}
                </Select.Option>
              ))}
          </Select>
        </div>
      </Modal>
    </div>
  );
}
