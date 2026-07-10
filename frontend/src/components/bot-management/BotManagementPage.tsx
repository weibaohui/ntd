import { useState, useEffect, useCallback, useRef } from 'react';
import { Spin, Empty, Button, Space, Typography, Modal, message } from 'antd';
import { RobotOutlined, PlusOutlined, EyeOutlined } from '@ant-design/icons';
import QRCode from 'qrcode';
import { PageCard } from '@/components/common/PageCard';
import { useIsMobile } from '@/hooks/useIsMobile';
import * as db from '@/utils/database';
import type { AgentBot, ProjectDirectory } from '@/utils/database';
import { BotConfigDrawer } from './BotConfigDrawer';
import { BotListTable } from './BotListTable';
import { BotListCards } from './BotListCards';

const { Title } = Typography;

interface BotManagementPageProps {}

export function BotManagementPage({}: BotManagementPageProps) {
  const isMobile = useIsMobile();
  const [loading, setLoading] = useState(true);
  const [bots, setBots] = useState<AgentBot[]>([]);
  const [workspaces, setWorkspaces] = useState<ProjectDirectory[]>([]);
  const [configDrawerOpen, setConfigDrawerOpen] = useState(false);
  const [selectedBot, setSelectedBot] = useState<AgentBot | null>(null);

  // 绑定飞书智能体的状态
  const [binding, setBinding] = useState(false);
  const [bindModalOpen, setBindModalOpen] = useState(false);
  const [qrCodeUrl, setQrCodeUrl] = useState('');
  const [pollError, setPollError] = useState('');
  const [bindSuccess, setBindSuccess] = useState(false);
  const [feishuEventSource, setFeishuEventSource] = useState<EventSource | null>(null);
  const successTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const loadData = useCallback(async () => {
    setLoading(true);
    try {
      const [botList, workspaceList] = await Promise.all([
        db.getAgentBots(),
        db.getProjectDirectories(),
      ]);
      setBots(botList);
      setWorkspaces(workspaceList);
    } catch {}
    setLoading(false);
  }, []);

  useEffect(() => {
    loadData();
  }, [loadData]);

  const handleOpenConfig = (bot: AgentBot) => {
    setSelectedBot(bot);
    setConfigDrawerOpen(true);
  };

  const handleToggleEnabled = async (bot: AgentBot) => {
    try {
      const newConfig = { ...JSON.parse(bot.config), enabled: !bot.enabled };
      await db.updateAgentBotConfig(bot.id, JSON.stringify(newConfig));
      loadData();
    } catch {}
  };

  const handleDelete = async (bot: AgentBot) => {
    try {
      await db.deleteAgentBot(bot.id);
      loadData();
    } catch {}
  };

  const handleRefresh = () => {
    loadData();
  };

  const handleConfigChanged = () => {
    loadData();
    setConfigDrawerOpen(false);
  };

  // 绑定飞书智能体逻辑（从 WorkspaceAgentPanel 复用）
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

      // 绑定时不传 workspaceId，绑定的 Bot 默认不分配工作空间
      // 用户可以在配置抽屉中选择服务工作空间
      const eventSource = db.feishuPollSSE(
        beginRes.device_code,
        beginRes.interval,
        beginRes.expire_in,
        (pollRes) => {
          if (pollRes.success) {
            setBindSuccess(true);
            message.success(`绑定成功！Bot: ${pollRes.bot_name || 'Feishu Bot'}`);
            loadData();
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
      );
      setFeishuEventSource(eventSource);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : '启动绑定失败';
      setPollError(msg);
      setBinding(false);
    }
  };

  // 关闭绑定弹窗时清理 SSE 连接
  useEffect(() => {
    return () => {
      feishuEventSource?.close();
      if (successTimerRef.current) clearTimeout(successTimerRef.current);
    };
  }, [feishuEventSource]);

  return (
    <PageCard icon={<RobotOutlined />} title="智能体管理中心">
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 20 }}>
        <div>
          <Title level={4} style={{ margin: 0 }}>智能体管理</Title>
          <p style={{ margin: '4px 0 0', color: 'var(--color-text-secondary)', fontSize: 13 }}>
            统一管理所有智能体，配置推送规则和白名单
          </p>
        </div>
        <Space>
          <Button type="text" size="small" icon={<EyeOutlined />} onClick={handleRefresh}>刷新</Button>
          <Button type="primary" size="small" icon={<PlusOutlined />} onClick={handleStartBind}>
            绑定智能体
          </Button>
        </Space>
      </div>

      {loading ? (
        <div style={{ display: 'flex', justifyContent: 'center', padding: 48 }}>
          <Spin />
        </div>
      ) : bots.length === 0 ? (
        <Empty description="暂无智能体，点击上方按钮绑定" />
      ) : isMobile ? (
        <BotListCards
          bots={bots}
          workspaces={workspaces}
          onOpenConfig={handleOpenConfig}
          onToggleEnabled={handleToggleEnabled}
          onDelete={handleDelete}
        />
      ) : (
        <BotListTable
          bots={bots}
          workspaces={workspaces}
          onOpenConfig={handleOpenConfig}
          onToggleEnabled={handleToggleEnabled}
          onDelete={handleDelete}
        />
      )}

      <BotConfigDrawer
        open={configDrawerOpen}
        bot={selectedBot}
        workspaces={workspaces}
        onClose={() => setConfigDrawerOpen(false)}
        onChanged={handleConfigChanged}
      />

      {/* 绑定飞书智能体的二维码弹窗 */}
      <Modal
        title="绑定飞书智能体"
        open={bindModalOpen}
        onCancel={() => {
          setBindModalOpen(false);
          setQrCodeUrl('');
          setPollError('');
          setBindSuccess(false);
          feishuEventSource?.close();
        }}
        footer={null}
        width={400}
        centered
      >
        <div style={{ textAlign: 'center', padding: '16px 0' }}>
          {pollError && <div style={{ marginBottom: 16, color: '#ff4d4f', fontSize: 13 }}>{pollError}</div>}
          {bindSuccess && <div style={{ marginBottom: 16, color: '#52c41a', fontSize: 14, fontWeight: 500 }}>✓ 绑定成功！</div>}
          {binding && !qrCodeUrl && <Spin tip="正在生成二维码..." />}
          {qrCodeUrl && (
            <img
              src={qrCodeUrl}
              alt="飞书绑定二维码"
              style={{ width: 256, height: 256, border: '1px solid #d9d9d9', borderRadius: 4 }}
            />
          )}
          {qrCodeUrl && !bindSuccess && (
            <p style={{ marginTop: 16, color: 'var(--color-text-secondary)', fontSize: 13 }}>
              请使用飞书 App 扫描二维码完成绑定
            </p>
          )}
        </div>
      </Modal>
    </PageCard>
  );
}