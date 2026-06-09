import { useState, useEffect } from 'react';
import { Select, Button, List, Empty, Spin, Tag, Popconfirm, message, Modal } from 'antd';
import { LinkOutlined, DisconnectOutlined, FolderOutlined, RobotOutlined } from '@ant-design/icons';
import * as db from '@/utils/database';
import { PENDING_CHAT_ID } from '@/utils/database/bots';
import type { AgentBot, FeishuProjectBindingItem } from '@/utils/database/bots';
import type { ProjectDirectory } from '@/utils/database';

/** 项目绑定管理面板 — 管理飞书聊天与项目目录的绑定关系 */
export function ProjectBindsTab() {
  const [bindings, setBindings] = useState<FeishuProjectBindingItem[]>([]);
  const [bots, setBots] = useState<AgentBot[]>([]);
  const [directories, setDirectories] = useState<ProjectDirectory[]>([]);
  const [loading, setLoading] = useState(false);
  const [selectedBotId, setSelectedBotId] = useState<number | undefined>(undefined);
  const [selectedDirId, setSelectedDirId] = useState<number | undefined>(undefined);
  const [bindModalOpen, setBindModalOpen] = useState(false);
  const [binding, setBinding] = useState(false);

  const loadAll = async () => {
    setLoading(true);
    try {
      const [b, d] = await Promise.all([
        db.getAgentBots(),
        db.getProjectDirectories(),
      ]);
      setBots(b.filter(bot => bot.bot_type === 'feishu'));
      setDirectories(d);

      if (selectedBotId !== undefined) {
        setBindings(await db.getFeishuBindings(selectedBotId));
      } else {
        setBindings(await db.getFeishuBindings());
      }
    } catch (err: any) {
      message.error('加载数据失败: ' + (err?.message || String(err)));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => { loadAll(); }, []);

  const handleBotChange = async (botId: number | undefined) => {
    setSelectedBotId(botId);
    try {
      if (botId !== undefined) {
        setBindings(await db.getFeishuBindings(botId));
      } else {
        setBindings(await db.getFeishuBindings());
      }
    } catch (err: any) {
      message.error('加载绑定列表失败');
    }
  };

  const handleCreateBinding = async () => {
    if (selectedBotId === undefined) {
      message.error('请选择 Bot');
      return;
    }
    if (selectedDirId === undefined) {
      message.error('请选择项目目录');
      return;
    }

    setBinding(true);
    try {
      // Create binding via API (auto-creates Todo inside)
      await db.createFeishuBinding({
        bot_id: selectedBotId,
        chat_id: PENDING_CHAT_ID, // placeholder — set via /bind on Feishu
        chat_type: 'p2p',
        project_dir_id: selectedDirId,
      });
      message.success('绑定已创建（请在飞书中使用 /bind 命令绑定具体聊天）');
      setBindModalOpen(false);
      handleBotChange(selectedBotId);
    } catch (err: any) {
      message.error('创建绑定失败: ' + (err?.message || String(err)));
    } finally {
      setBinding(false);
    }
  };

  const handleDeleteBinding = async (id: number) => {
    try {
      await db.deleteFeishuBinding(id);
      message.success('已解绑');
      handleBotChange(selectedBotId);
    } catch (err: any) {
      message.error('解绑失败: ' + (err?.message || String(err)));
    }
  };

  const chatTypeLabel = (t: string) => t === 'p2p' ? '私聊' : '群聊';
  const isPending = (item: FeishuProjectBindingItem) => item.chat_id === PENDING_CHAT_ID;
  const statusTag = (s: string, pending: boolean) => {
    if (pending) return <Tag color="orange">待绑定</Tag>;
    if (s === 'running') return <Tag color="green">运行中</Tag>;
    return <Tag>空闲</Tag>;
  };

  return (
    <Spin spinning={loading}>
      {/* Bot 选择器 */}
      <div style={{ marginBottom: 16, display: 'flex', gap: 12, alignItems: 'center' }}>
        <div style={{ fontWeight: 500, whiteSpace: 'nowrap' }}>选择 Bot：</div>
        <Select
          placeholder="选择飞书 Bot"
          allowClear
          style={{ width: 280 }}
          value={selectedBotId}
          onChange={handleBotChange}
          options={bots.map(b => ({
            label: `${b.bot_name} (${b.app_id})`,
            value: b.id,
          }))}
        />
        <Button type="primary" icon={<LinkOutlined />} onClick={() => setBindModalOpen(true)}>
          新建绑定
        </Button>
      </div>

      {/* 绑定列表 */}
      {bindings.length === 0 ? (
        <Empty description={selectedBotId ? '该 Bot 暂无项目绑定' : '暂无绑定，请先选择 Bot'} image={Empty.PRESENTED_IMAGE_SIMPLE} />
      ) : (
        <List
          dataSource={bindings}
          renderItem={(item) => (
            <List.Item
              style={{
                padding: '12px',
                background: 'var(--color-bg)',
                borderRadius: 6,
                marginBottom: 8,
                border: '1px solid var(--color-border-light)',
              }}
            >
              <div style={{ display: 'flex', alignItems: 'center', gap: 12, flex: 1, minWidth: 0 }}>
                <RobotOutlined style={{ fontSize: 18, color: '#52c41a' }} />
                <div style={{ flex: 1, minWidth: 0 }}>
                  <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                    <span style={{ fontWeight: 500 }}>
                      {item.project_name || item.project_path || '(未知项目)'}
                    </span>
                    {statusTag(item.status, isPending(item))}
                  </div>
                  <div style={{ fontSize: 12, color: 'var(--color-text-secondary)', marginTop: 2 }}>
                    <FolderOutlined style={{ marginRight: 4 }} />
                    {item.project_path || '(无路径)'}
                    <span style={{ margin: '0 8px' }}>|</span>
                    {isPending(item) ? (
                      <span style={{ color: 'var(--color-warning)' }}>未绑定聊天（使用飞书 /bind 命令绑定）</span>
                    ) : (
                      <>{chatTypeLabel(item.chat_type)} · {item.chat_id}</>
                    )}
                    <span style={{ margin: '0 8px' }}>|</span>
                    Todo #{item.todo_id}
                    {item.session_id && <span> · Session: {item.session_id.slice(0, 8)}…</span>}
                  </div>
                </div>
                <Popconfirm
                  title="确认解绑"
                  description={`解除与「${item.project_name || item.project_path}」的绑定？`}
                  onConfirm={() => handleDeleteBinding(item.id)}
                >
                  <Button type="text" danger icon={<DisconnectOutlined />} size="small" />
                </Popconfirm>
              </div>
            </List.Item>
          )}
        />
      )}

      {/* 新建绑定 Modal */}
      <Modal
        title="新建项目绑定"
        open={bindModalOpen}
        onCancel={() => setBindModalOpen(false)}
        onOk={handleCreateBinding}
        confirmLoading={binding}
        okText="创建"
      >
        <div style={{ marginBottom: 12 }}>选择要绑定的项目目录：</div>
        <Select
          placeholder="选择项目目录"
          style={{ width: '100%' }}
          value={selectedDirId}
          onChange={setSelectedDirId}
          options={directories.map(d => ({
            label: `${d.name || '(未命名)'} — ${d.path}`,
            value: d.id,
          }))}
        />
        <div style={{ marginTop: 12, fontSize: 13, color: 'var(--color-text-secondary)' }}>
          创建绑定后，请在对应的飞书聊天中使用 <code>/bind &lt;项目名称&gt;</code> 命令完成绑定。
        </div>
      </Modal>
    </Spin>
  );
}
