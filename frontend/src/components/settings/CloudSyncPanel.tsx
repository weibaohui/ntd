import { useState, useEffect, useCallback } from 'react';
import { Card, Form, Input, Button, Select, Space, Table, Tag, message, Modal, Divider, Alert, Typography, Popconfirm } from 'antd';
import { CloudOutlined, SyncOutlined, SaveOutlined, CheckCircleFilled, ExclamationCircleFilled, DeleteOutlined } from '@ant-design/icons';
import * as syncApi from '../../utils/database/sync';
import { getDeviceName } from '../../utils/device';

const { Text, Paragraph } = Typography;

export function CloudSyncPanel() {
  const [loading, setLoading] = useState(false);
  const [syncing, setSyncing] = useState(false);
  const [deviceModalOpen, setDeviceModalOpen] = useState(false);
  const [deviceLoading, setDeviceLoading] = useState(false);
  const [syncHistory, setSyncHistory] = useState<syncApi.SyncRecord[]>([]);
  const [statusInfo, setStatusInfo] = useState<syncApi.SyncStatusResponse | null>(null);
  const [configForm] = Form.useForm();

  // 加载配置和状态
  const loadData = useCallback(async () => {
    try {
      const [status, config] = await Promise.all([
        syncApi.getCloudSyncStatus(),
        syncApi.getCloudConfig(),
      ]);
      setStatusInfo(status);
      configForm.setFieldsValue({
        server_url: config.server_url,
        token: config.token || '',
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
      await syncApi.saveCloudConfig({
        server_url: values.server_url,
        token: values.token || null,
        default_conflict_mode: values.default_conflict_mode,
      });
      message.success('配置已保存');
      await loadData();
    } catch (err: any) {
      message.error('保存失败: ' + (err?.message || String(err)));
    } finally {
      setLoading(false);
    }
  };

  // 创建设备
  const handleCreateDevice = async () => {
    if (!statusInfo?.authenticated) {
      message.warning('请先配置 Token');
      return;
    }
    try {
      setDeviceLoading(true);
      const deviceName = getDeviceName();
      await syncApi.cloudCreateDevice(deviceName);
      message.success('设备注册成功');
      setDeviceModalOpen(false);
      await loadData();
    } catch (err: any) {
      message.error('设备注册失败: ' + (err?.message || String(err)));
    } finally {
      setDeviceLoading(false);
    }
  };

  // 清除 Token
  const handleClearToken = async () => {
    try {
      await syncApi.saveCloudConfig({
        server_url: statusInfo?.server_url,
        token: '',
      });
      message.success('已清除 Token');
      await loadData();
    } catch (err: any) {
      message.error('清除失败: ' + (err?.message || String(err)));
    }
  };

  // 执行同步
  const handleSync = async (direction: 'push' | 'pull') => {
    if (!statusInfo?.authenticated) {
      message.warning('请先配置 Token');
      return;
    }

    try {
      setSyncing(true);
      message.info(direction === 'push' ? '向上同步功能开发中...' : '向下同步功能开发中...');
      await loadSyncHistory();
    } catch (err: any) {
      message.error('同步失败: ' + (err?.message || String(err)));
    } finally {
      setSyncing(false);
    }
  };

  const isAuthenticated = statusInfo?.authenticated ?? false;
  const isConnected = statusInfo?.connected ?? false;

  const columns = [
    {
      title: '时间',
      dataIndex: 'created_at',
      key: 'created_at',
      width: 160,
      render: (text: string) => text ? new Date(text).toLocaleString('zh-CN') : '-',
    },
    {
      title: '方向',
      dataIndex: 'direction',
      key: 'direction',
      width: 80,
      render: (dir: string) => dir === 'push'
        ? <Tag color="blue">推送</Tag>
        : <Tag color="green">拉取</Tag>,
    },
    {
      title: '冲突模式',
      dataIndex: 'conflict_mode',
      key: 'conflict_mode',
      width: 100,
      render: (mode: string) => {
        const map: Record<string, string> = { overwrite: '覆盖', skip: '跳过', rename: '重命名' };
        return map[mode] || mode;
      },
    },
    {
      title: '状态',
      dataIndex: 'status',
      key: 'status',
      width: 80,
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
    <div style={{ padding: 24 }}>
      <Card
        title={
          <Space>
            <CloudOutlined />
            <span>云端同步</span>
          </Space>
        }
        extra={
          <Space>
            {isAuthenticated && (
              <>
                <Button
                  icon={<SyncOutlined />}
                  onClick={() => handleSync('push')}
                  loading={syncing}
                >
                  向上同步
                </Button>
                <Button
                  icon={<SyncOutlined style={{ transform: 'rotate(180deg)' }} />}
                  onClick={() => handleSync('pull')}
                  loading={syncing}
                >
                  从云上拉取
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
                  <Space>
                    <CheckCircleFilled style={{ color: '#52c41a' }} />
                    已连接到云端服务器 ({statusInfo?.server_url})
                    {statusInfo?.device_id && <span>设备ID: {statusInfo.device_id}</span>}
                    {statusInfo?.last_sync_at && <span>最后同步: {new Date(statusInfo.last_sync_at).toLocaleString('zh-CN')}</span>}
                  </Space>
                }
                type="success"
                showIcon
              />
            ) : (
              <Alert
                message={
                  <Space>
                    <ExclamationCircleFilled style={{ color: '#faad14' }} />
                    已连接到云端服务器 ({statusInfo?.server_url})，但未配置 Token
                  </Space>
                }
                type="warning"
                showIcon
              />
            )
          ) : (
            <Alert
              message="未配置云端服务器地址"
              description="请在下方配置服务器地址和 Token。"
              type="info"
              showIcon
            />
          )}
        </div>

        {/* 配置表单 */}
        <Form form={configForm} layout="vertical">
          <Form.Item
            label="云端服务器地址"
            name="server_url"
            rules={[{ required: true, message: '请输入服务器地址' }]}
          >
            <Input placeholder="http://localhost:8089" />
          </Form.Item>

          <Form.Item
            label="Token"
            name="token"
            rules={[{ required: true, message: '请输入 Token' }]}
          >
            <Input.Password placeholder="从云端获取的 JWT Token" />
          </Form.Item>

          <Form.Item
            label="默认冲突解决模式"
            name="default_conflict_mode"
          >
            <Select>
              <Select.Option value="overwrite">
                <Space>
                  <span>覆盖</span>
                  <Text type="secondary">（以云端数据为准）</Text>
                </Space>
              </Select.Option>
              <Select.Option value="skip">
                <Space>
                  <span>跳过</span>
                  <Text type="secondary">（保留本地数据）</Text>
                </Space>
              </Select.Option>
              <Select.Option value="rename">
                <Space>
                  <span>重命名</span>
                  <Text type="secondary">（冲突项重命名保留）</Text>
                </Space>
              </Select.Option>
            </Select>
          </Form.Item>

          <Form.Item>
            <Space>
              <Button
                type="primary"
                icon={<SaveOutlined />}
                onClick={handleSaveConfig}
                loading={loading}
              >
                保存配置
              </Button>
              {isAuthenticated && statusInfo?.device_id && (
                <Popconfirm
                  title="确定要清除 Token 吗？"
                  onConfirm={handleClearToken}
                  okText="确定"
                  cancelText="取消"
                >
                  <Button danger icon={<DeleteOutlined />}>
                    清除 Token
                  </Button>
                </Popconfirm>
              )}
            </Space>
          </Form.Item>

          <Divider style={{ margin: '16px 0' }} />

          <Paragraph type="secondary" style={{ marginBottom: 8 }}>
            如果还没有设备 ID，请先在云端创建设备后填入 Token，然后点击下方按钮注册本设备。
          </Paragraph>
          <Button
            onClick={() => setDeviceModalOpen(true)}
            disabled={!isAuthenticated}
          >
            注册本设备
          </Button>
        </Form>

        <Divider>同步历史</Divider>

        <Table
          columns={columns}
          dataSource={syncHistory}
          rowKey="id"
          size="small"
          pagination={{ pageSize: 10, showSizeChanger: false }}
          locale={{ emptyText: '暂无同步记录' }}
        />
      </Card>

      {/* 注册设备弹窗 */}
      <Modal
        title="注册本设备"
        open={deviceModalOpen}
        onCancel={() => setDeviceModalOpen(false)}
        onOk={handleCreateDevice}
        confirmLoading={deviceLoading}
        okText="注册"
        cancelText="取消"
      >
        <Paragraph>
          确定要注册本设备吗？<br />
          设备名称: <strong>{getDeviceName()}</strong>
        </Paragraph>
      </Modal>
    </div>
  );
}
