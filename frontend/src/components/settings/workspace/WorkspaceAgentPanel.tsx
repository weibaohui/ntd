import { useState, useEffect, useRef } from 'react';
import { Table, Button, Space, Switch, Popconfirm, message, Modal, Select, Tooltip, Spin } from 'antd';
import { DeleteOutlined, SwapOutlined, QrcodeOutlined, FileTextOutlined } from '@ant-design/icons';
import QRCode from 'qrcode';
import type { ColumnsType } from 'antd/es/table';
import * as db from '@/utils/database';
import type { AgentBot, ProjectDirectory } from '@/utils/database';
import { BotDetailPage } from '@/components/settings/bot/BotDetailPage';

interface WorkspaceAgentPanelProps {
  workspaceId: number;
  onBotChanged?: () => void;
  // 外部控制 bot 选择状态（用于 WorkspaceMessageConfigPage）
  activeBot?: AgentBot | null;
  onSelectBot?: (bot: AgentBot) => void;
  onSelectBotForHistory?: (bot: AgentBot) => void;
  onBotBack?: () => void;
  autoShowHistory?: boolean;
}

export function WorkspaceAgentPanel({ workspaceId, onBotChanged, activeBot, onSelectBot, onSelectBotForHistory, onBotBack, autoShowHistory }: WorkspaceAgentPanelProps) {
  const [bots, setBots] = useState<AgentBot[]>([]);
  const [workspaces, setWorkspaces] = useState<ProjectDirectory[]>([]);
  const [loading, setLoading] = useState(false);
  const [moveModalVisible, setMoveModalVisible] = useState(false);
  const [movingBotId, setMovingBotId] = useState<number | null>(null);
  const [targetWorkspaceId, setTargetWorkspaceId] = useState<number | null>(null);
  // 内部状态：选中的 bot，显示详情页
  const [selectedBot, setSelectedBot] = useState<AgentBot | null>(null);
  // 点击消息记录时，直接打开详情页并默认展开消息记录
  const [selectedBotForHistory, setSelectedBotForHistory] = useState<AgentBot | null>(null);

  // 绑定飞书状态
  const [binding, setBinding] = useState(false);
  const [bindModalOpen, setBindModalOpen] = useState(false);
  const [qrCodeUrl, setQrCodeUrl] = useState('');
  const [pollError, setPollError] = useState('');
  const [bindSuccess, setBindSuccess] = useState(false);
  const [feishuEventSource, setFeishuEventSource] = useState<EventSource | null>(null);
  const successTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // 加载当前工作空间的智能体列表，全局Bot管理已迁移到 BotManagementPage
  const loadBots = () => {
    setLoading(true);
    // 只获取当前工作空间的 Bot 列表（通过前端过滤）
    // 跨空间管理请使用左侧菜单「配置 → 智能体」入口
    Promise.all([
      db.getAgentBots(),
      db.getProjectDirectories(),
    ])
      .then(([botsData, dirsData]) => {
        // 只保留属于当前工作空间的 Bot
        setBots(botsData.filter(b => b.workspace_id === workspaceId));
        setWorkspaces(dirsData);
      })
      .catch((err: any) => message.error('加载智能体失败: ' + (err?.message || String(err))))
      .finally(() => setLoading(false));
  };

  // 外部传入 bot 时直接渲染 BotDetailPage
  if (activeBot) {
    return (
      <BotDetailPage
        bot={activeBot}
        onBack={onBotBack || (() => { setSelectedBot(null); setSelectedBotForHistory(null); })}
        onRefresh={() => { loadBots(); onBotChanged?.(); }}
        autoShowHistory={autoShowHistory || false}
      />
    );
  }

  // 内部状态选中的 bot（外部模式不使用）
  const internalActiveBot = selectedBotForHistory || selectedBot;
  if (internalActiveBot) {
    return (
      <BotDetailPage
        bot={internalActiveBot}
        onBack={() => { setSelectedBot(null); setSelectedBotForHistory(null); }}
        onRefresh={() => { loadBots(); onBotChanged?.(); }}
        autoShowHistory={!!selectedBotForHistory}
      />
    );
  }

  // 开始绑定飞书
  const handleStartBind = async () => {
    if (successTimerRef.current) clearTimeout(successTimerRef.current);
    if (feishuEventSource) feishuEventSource.close();

    setBinding(true);
    setBindSuccess(false);
    setPollError('');
    setQrCodeUrl('');
    setBindModalOpen(true);

    try {
      const initRes = await db.feishuInit();
      if (!initRes.supported) {
        setPollError('当前环境不支持 client_secret 认证');
        setBinding(false);
        return;
      }

      const beginRes = await db.feishuBegin();
      const qrDataUrl = await QRCode.toDataURL(beginRes.qr_url, { width: 256, margin: 2 });
      setQrCodeUrl(qrDataUrl);

      const eventSource = db.feishuPollSSE(
        beginRes.device_code,
        beginRes.interval,
        beginRes.expire_in,
        (pollRes) => {
          if (pollRes.success) {
            setBindSuccess(true);
            message.success(`绑定成功！Bot: ${pollRes.bot_name || 'Feishu Bot'}`);
            // 绑定成功后刷新列表，新 bot 会自动出现在当前 workspace
            loadBots();
            onBotChanged?.();
            successTimerRef.current = setTimeout(() => {
              setBindModalOpen(false);
              setQrCodeUrl('');
            }, 2000);
          } else {
            const errMsg = pollRes.error === 'access_denied' ? '用户拒绝了绑定请求'
              : pollRes.error === 'expired_token' ? '二维码已过期，请重新绑定'
              : '绑定超时，请重试';
            setPollError(errMsg);
          }
          setBinding(false);
        },
        (error) => {
          setPollError(error || 'SSE 连接失败');
          setBinding(false);
        },
        workspaceId,
      );
      setFeishuEventSource(eventSource);
    } catch (err: any) {
      setPollError(err?.message || '启动绑定失败');
      setBinding(false);
    }
  };

  // 关闭绑定弹窗时清理
  useEffect(() => {
    return () => {
      feishuEventSource?.close();
      if (successTimerRef.current) clearTimeout(successTimerRef.current);
    };
  }, [feishuEventSource]);

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
      width: 180,
      render: (_: any, bot: AgentBot) => (
        <Space>
          <Button
            type="text"
            size="small"
            onClick={() => onSelectBot ? onSelectBot(bot) : setSelectedBot(bot)}
          >
            详情
          </Button>
          <Button
            type="text"
            size="small"
            icon={<FileTextOutlined />}
            onClick={() => onSelectBotForHistory ? onSelectBotForHistory(bot) : setSelectedBotForHistory(bot)}
            title="查看消息记录"
          />
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
            <Button type="text" size="small" icon={<DeleteOutlined />} />
          </Popconfirm>
        </Space>
      ),
    },
  ];

  return (
    <div>
      <div style={{ marginBottom: 16, display: 'flex', alignItems: 'center', gap: 12 }}>
        <span style={{ color: '#666' }}>
          当前工作空间共有 {bots.length} 个智能体
        </span>
        <Button type="primary" icon={<QrcodeOutlined />} onClick={handleStartBind} loading={binding} size="small">
          绑定飞书
        </Button>
      </div>

      <Table
        columns={columns}
        dataSource={bots}
        rowKey="id"
        loading={loading}
        pagination={false}
        locale={{ emptyText: '暂无智能体' }}
        scroll={{ x: 'max-content' }}
        size="small"
      />

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

      {/* 绑定飞书 Modal */}
      <Modal
        title={<Space><QrcodeOutlined />绑定飞书智能体</Space>}
        open={bindModalOpen}
        onCancel={() => { setBindModalOpen(false); setQrCodeUrl(''); setPollError(''); setBindSuccess(false); }}
        footer={null}
        width={400}
        centered
      >
        <div style={{ textAlign: 'center', padding: '16px 0' }}>
          {pollError && <div style={{ marginBottom: 16, color: '#ff4d4f', fontSize: 13 }}>{pollError}</div>}
          {bindSuccess ? (
            <div style={{ color: '#52c41a', fontSize: 48, marginBottom: 16 }}>✓</div>
          ) : qrCodeUrl ? (
            <div style={{ marginBottom: 16 }}>
              <img src={qrCodeUrl} alt="QR Code" style={{ width: '100%', maxWidth: 200, height: 'auto' }} />
              <div style={{ marginTop: 12, color: 'var(--color-text-secondary)', fontSize: 13 }}>请使用飞书 App 扫描二维码绑定</div>
              <div style={{ marginTop: 6, fontSize: 12, color: 'var(--color-text-tertiary)' }}>二维码有效期 10 分钟，请尽快完成</div>
            </div>
          ) : (
            <Spin size="large" />
          )}
          {binding && !qrCodeUrl && <div style={{ marginTop: 16, color: 'var(--color-text-secondary)', fontSize: 13 }}>正在生成二维码...</div>}
        </div>
      </Modal>
    </div>
  );
}
