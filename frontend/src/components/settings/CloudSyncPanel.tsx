import { useState, useEffect, useCallback, useRef } from 'react';
import { Card, Form, Input, Button, Space, Table, Tag, message, Divider, Alert, Modal, Checkbox, Radio } from 'antd';
import { CloudOutlined, SyncOutlined, SaveOutlined, CheckCircleFilled, ExclamationCircleFilled } from '@ant-design/icons';
import * as syncApi from '../../utils/database/sync';
import './CloudSyncPanel.css';

export function CloudSyncPanel() {
  const [loading, setLoading] = useState(false);
  const [syncing, setSyncing] = useState(false);
  const [syncHistory, setSyncHistory] = useState<syncApi.SyncRecord[]>([]);
  const [statusInfo, setStatusInfo] = useState<syncApi.SyncStatusResponse | null>(null);
  const [configForm] = Form.useForm();
  const [hasToken, setHasToken] = useState(false);
  const tokenRef = useRef('');

  // 加载配置和状态
  const loadData = useCallback(async () => {
    try {
      const [status, config] = await Promise.all([
        syncApi.getCloudSyncStatus(),
        syncApi.getCloudConfig(),
      ]);
      setStatusInfo(status);
      setHasToken(config.has_token ?? false);
      // 保持当前输入的 token 不变
      configForm.setFieldsValue({
        server_url: config.server_url,
        sync_token: tokenRef.current || '',
        default_conflict_mode: config.default_conflict_mode,
      });
    } catch (err) {
      console.error('加载云端配置失败:', err);
    }
  }, [configForm]);

  // 加载同步历史
  const loadSyncHistory = useCallback(async () => {
    try {
      const records = await syncApi.getSyncRecords({ limit: 20 });
      setSyncHistory(records);
    } catch (err) {
      console.error('加载同步历史失败:', err);
    }
  }, []);

  useEffect(() => {
    loadData();
    loadSyncHistory();
  }, [loadData, loadSyncHistory]);

  // 保存配置
  const handleSaveConfig = async () => {
    try {
      const values = await configForm.validateFields();
      setLoading(true);
      const tokenToSave = values.sync_token || values.sync_token === '' ? values.sync_token : tokenRef.current;
      await syncApi.saveCloudConfig({
        server_url: values.server_url,
        sync_token: tokenToSave,
      });
      tokenRef.current = values.sync_token || '';
      setHasToken(!!values.sync_token);
      message.success('配置已保存');
    } catch (err: any) {
      message.error('保存失败: ' + (err?.message || String(err)));
    } finally {
      setLoading(false);
    }
  };

  // 执行同步
  const handleSync = async (direction: 'push' | 'pull') => {
    if (!statusInfo?.authenticated && !hasToken) {
      message.warning('请先配置同步 Token');
      return;
    }

    // 弹出确认框选择冲突模式
    let selectedMode = 'overwrite';
    let dryRun = false;

    // 根据方向显示不同的策略文案
    const isPush = direction === 'push';
    const modeOptions = isPush ? [
      { value: 'overwrite', label: '覆盖（以本地数据为准，覆盖云端）' },
      { value: 'skip', label: '跳过（保留云端，忽略本地冲突项）' },
      { value: 'rename', label: '重命名（避免冲突，本地项重命名保留）' },
    ] : [
      { value: 'overwrite', label: '覆盖（以云端数据为准，覆盖本地）' },
      { value: 'skip', label: '跳过（保留本地，忽略云端冲突项）' },
      { value: 'rename', label: '重命名（避免冲突，云端项重命名保留）' },
    ];

    Modal.confirm({
      title: isPush ? '确认向上同步（推送至云端）' : '确认向下同步（拉取至本地）',
      content: (
        <div>
          <p>冲突解决策略：</p>
          <Radio.Group
            value={selectedMode}
            onChange={e => { selectedMode = e.target.value; }}
            style={{ marginBottom: 16 }}
          >
            {modeOptions.map(opt => (
              <Radio key={opt.value} value={opt.value} style={{ display: 'block', marginBottom: 8 }}>{opt.label}</Radio>
            ))}
          </Radio.Group>
          <Checkbox
            onChange={e => { dryRun = e.target.checked; }}
          >
            预览模式 (Dry Run)
          </Checkbox>
        </div>
      ),
      okText: '执行同步',
      cancelText: '取消',
      onOk: async () => {
        try {
          setSyncing(true);
          let result: syncApi.SyncResult;
          if (direction === 'push') {
            result = await syncApi.syncPush({ conflict_mode: selectedMode, dry_run: dryRun });
          } else {
            result = await syncApi.syncPull({ conflict_mode: selectedMode, dry_run: dryRun });
          }

          if (result.success) {
            const msg = dryRun ? '预览成功' : '同步成功';
            if (direction === 'push') {
              message.success(`${msg}：推送 ${result.pushed_count} 条`);
            } else {
              message.success(`${msg}：拉取 ${result.pulled_count} 条`);
            }
          } else {
            message.error('同步失败：' + (result.errors[0] || '未知错误'));
          }
          await loadSyncHistory();
        } catch (err: any) {
          message.error('同步失败：' + (err?.message || String(err)));
        } finally {
          setSyncing(false);
        }
      },
    });
  };

  const isAuthenticated = statusInfo?.authenticated ?? hasToken;
  const isConnected = statusInfo?.connected ?? false;

  const columns = [
    {
      title: '时间',
      dataIndex: 'created_at',
      key: 'created_at',
      width: 140,
      render: (text: string) => text ? new Date(text).toLocaleString('zh-CN') : '-',
    },
    {
      title: '方向',
      dataIndex: 'direction',
      key: 'direction',
      width: 60,
      render: (dir: string) => dir === 'push'
        ? <Tag color="blue">推送</Tag>
        : <Tag color="green">拉取</Tag>,
    },
    {
      title: '状态',
      dataIndex: 'status',
      key: 'status',
      width: 60,
      render: (status: string) => {
        const color = status === 'success' ? 'green' : status === 'failed' ? 'red' : 'orange';
        const text = status === 'success' ? '成功' : status === 'failed' ? '失败' : '预览';
        return <Tag color={color}>{text}</Tag>;
      },
    },
    {
      title: '详情',
      dataIndex: 'details',
      key: 'details',
      ellipsis: true,
    },
  ];

  return (
    <div style={{ padding: 16 }} className="cloud-sync-panel">
      <Card
        title={
          <Space>
            <CloudOutlined />
            <span>云端同步</span>
          </Space>
        }
        extra={
          <Space size="small">
            {isAuthenticated && (
              <>
                <Button
                  size="small"
                  icon={<SyncOutlined />}
                  onClick={() => handleSync('push')}
                  loading={syncing}
                >
                  推送
                </Button>
                <Button
                  size="small"
                  icon={<SyncOutlined style={{ transform: 'rotate(180deg)' }} />}
                  onClick={() => handleSync('pull')}
                  loading={syncing}
                >
                  拉取
                </Button>
              </>
            )}
          </Space>
        }
      >
        {/* 连接状态 */}
        <div style={{ marginBottom: 16 }}>
          {isConnected ? (
            isAuthenticated ? (
              <Alert
                message={
                  <Space size="small">
                    <CheckCircleFilled style={{ color: '#52c41a' }} />
                    <span className="status-text">已连接 ({statusInfo?.server_url})</span>
                    {statusInfo?.last_sync_at && (
                      <span className="status-text">最后同步: {new Date(statusInfo.last_sync_at).toLocaleString('zh-CN')}</span>
                    )}
                  </Space>
                }
                type="success"
                showIcon
              />
            ) : (
              <Alert
                message={
                  <Space size="small">
                    <ExclamationCircleFilled style={{ color: '#faad14' }} />
                    <span className="status-text">已连接但未配置 Token</span>
                  </Space>
                }
                type="warning"
                showIcon
              />
            )
          ) : (
            <Alert
              message="未配置云端服务器地址"
              description="请在下方配置服务器地址和同步 Token。"
              type="info"
              showIcon
            />
          )}
        </div>

        {/* 配置表单 */}
        <Form form={configForm} layout="vertical" size="small">
          <Form.Item
            label="服务器地址"
            name="server_url"
            rules={[{ required: true, message: '请输入服务器地址' }]}
          >
            <Input placeholder="http://localhost:8089" />
          </Form.Item>

          <Form.Item
            label="同步 Token"
            name="sync_token"
            rules={[{ required: true, message: '请输入同步 Token' }]}
          >
            <Input.Password placeholder="ntd_xxx 格式的同步 Token" />
          </Form.Item>

          <Form.Item style={{ marginBottom: 0 }}>
            <Button
              type="primary"
              icon={<SaveOutlined />}
              onClick={handleSaveConfig}
              loading={loading}
              size="small"
            >
              保存配置
            </Button>
          </Form.Item>
        </Form>

        <Divider style={{ margin: '16px 0' }}>同步历史</Divider>

        <Table
          columns={columns}
          dataSource={syncHistory}
          rowKey="id"
          size="small"
          pagination={{ pageSize: 10, showSizeChanger: false }}
          locale={{ emptyText: '暂无同步记录' }}
          scroll={{ x: 400 }}
        />
      </Card>
    </div>
  );
}
