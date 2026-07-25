// 工艺模板库页面：展示已同步的工艺模板列表，支持查看详情与安装为 Loop。

import { useEffect, useState } from 'react';
import {
  Card,
  Col,
  Row,
  Typography,
  Tag,
  Button,
  Empty,
  Spin,
  Modal,
  Select,
  message,
  Descriptions,
  Input,
  Alert,
  Space,
} from 'antd';
import { BuildOutlined, ReloadOutlined, EyeOutlined, DownloadOutlined, SearchOutlined } from '@ant-design/icons';
import bundledApi, { type ProcessTemplate, type ProcessTemplateDetail } from '@/api/bundled';
import { useProjectDirectories } from '@/utils/workspaceDisplay';

const { Title, Text, Paragraph } = Typography;

interface ProcessPageProps {
  workspaceId: number | null;
}

export function ProcessPage(_props: ProcessPageProps) {
  const { dirs: workspaces } = useProjectDirectories();
  const [processes, setProcesses] = useState<ProcessTemplate[]>([]);
  const [loading, setLoading] = useState(false);
  const [detail, setDetail] = useState<ProcessTemplateDetail | null>(null);
  const [installing, setInstalling] = useState<string | null>(null);
  const [searchText, setSearchText] = useState('');
  const [recommended, setRecommended] = useState<string[]>([]);
  const [installModal, setInstallModal] = useState<{ name: string; displayName: string } | null>(null);
  const [selectedWs, setSelectedWs] = useState<number | null>(null);

  const load = async () => {
    setLoading(true);
    try {
      const list = await bundledApi.getProcesses();
      setProcesses(list);
    } catch (e) {
      message.error('加载工艺模板失败');
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    load();
  }, []);

  const handleSearch = async (value: string) => {
    setSearchText(value);
    if (!value.trim()) { setRecommended([]); return; }
    try {
      const resp = await bundledApi.recommendProcesses(value);
      setRecommended(resp.recommendations.map((r: { template_name: string }) => r.template_name));
    } catch {
      // 推荐失败不影响正常使用。
    }
  };

  const handleShowDetail = async (name: string) => {
    try {
      const data = await bundledApi.getProcess(name);
      setDetail(data);
    } catch (e) {
      message.error('加载工艺模板详情失败');
    }
  };

  const handleInstall = (name: string, displayName: string) => {
    setInstallModal({ name, displayName });
  };

  const doInstall = async () => {
    if (!installModal || !selectedWs) { message.warning('请选择目标工作空间'); return; }
    setInstalling(installModal.name);
    try {
      const result = await bundledApi.installProcess(installModal.name, selectedWs);
      message.success(`已安装「${result.loop_name}」`);
      setInstallModal(null); setSelectedWs(null);
    } catch { message.error('安装失败'); }
    finally { setInstalling(null); }
  };

  const complexityColor = (complexity: string) => {
    switch (complexity) {
      case 'light':
        return 'green';
      case 'standard':
        return 'blue';
      case 'complex':
        return 'purple';
      default:
        return 'default';
    }
  };

  const complexityLabel = (complexity: string) => {
    switch (complexity) {
      case 'light':
        return '轻量';
      case 'standard':
        return '标准';
      case 'complex':
        return '复杂';
      default:
        return complexity;
    }
  };

  return (
    <div style={{ flex: 1, minWidth: 0, height: '100%', overflow: 'auto', padding: 24 }}>
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 24 }}>
        <Title level={3} style={{ margin: 0 }}>
          <BuildOutlined style={{ marginRight: 8 }} />
          工艺模板库
        </Title>
        <div style={{ display: 'flex', gap: 12, alignItems: 'center' }}>
          <Button icon={<ReloadOutlined />} onClick={load} loading={loading}>
            刷新
          </Button>
        </div>
      </div>

      <div style={{ display: 'flex', gap: 12, marginBottom: 16 }}>
        <Input.Search
          placeholder="搜索或描述任务，系统推荐合适工艺…"
          allowClear
          enterButton={<><SearchOutlined /> 推荐</>}
          size="large"
          onSearch={handleSearch}
          onChange={(e) => { if (!e.target.value) setRecommended([]); }}
          style={{ flex: 1 }}
        />
        <Select
          placeholder="复杂度筛选"
          allowClear
          style={{ width: 140 }}
          onChange={(val) => { setSearchText(val ? '' : searchText); }}
          options={[
            { label: '轻量', value: 'light' },
            { label: '标准', value: 'standard' },
            { label: '复杂', value: 'complex' },
          ]}
        />
      </div>

      {recommended.length > 0 && (
        <Alert
          type="info"
          showIcon
          message="系统推荐"
          description={`推荐使用：${recommended.join('、')}`}
          style={{ marginBottom: 16 }}
          closable
          onClose={() => setRecommended([])}
        />
      )}

      {loading && processes.length === 0 ? (
        <div style={{ textAlign: 'center', padding: 64 }}>
          <Spin size="large" />
        </div>
      ) : processes.length === 0 ? (
        <Empty description="暂无工艺模板，请先执行 bundled 同步" />
      ) : (
        <Row gutter={[16, 16]}>
          {processes
            .filter((p) => !searchText.trim() || p.name.includes(searchText) || p.display_name.includes(searchText) || p.description.includes(searchText))
            .map((p) => (
            <Col xs={24} sm={12} lg={8} key={p.id}>
              <Card
                hoverable
                title={p.display_name || p.name}
                extra={
                  <Space>
                    {recommended.includes(p.name) && <Tag color="gold">推荐</Tag>}
                    <Tag color={complexityColor(p.complexity)}>{complexityLabel(p.complexity)}</Tag>
                  </Space>
                }
                actions={[
                  <Button key="view" type="text" icon={<EyeOutlined />} onClick={() => handleShowDetail(p.name)}>
                    详情
                  </Button>,
                  <Button
                    key="install"
                    type="text"
                    icon={<DownloadOutlined />}
                    loading={installing === p.name}
                    onClick={() => handleInstall(p.name, p.display_name || p.name)}
                  >
                    安装
                  </Button>,
                ]}
              >
                <Paragraph ellipsis={{ rows: 2 }} type="secondary">
                  {p.description || '无描述'}
                </Paragraph>
                <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                  <Text type="secondary" style={{ fontSize: 12 }}>
                    版本 {p.version}
                  </Text>
                  <Tag>{p.category || '未分类'}</Tag>
                </div>
              </Card>
            </Col>
          ))}
        </Row>
      )}

      <Modal
        open={detail != null}
        title={detail?.display_name || detail?.name}
        width={720}
        footer={[
          <Button key="close" onClick={() => setDetail(null)}>
            关闭
          </Button>,
          <Button
            key="install"
            type="primary"
            icon={<DownloadOutlined />}
            loading={installing === detail?.name}
            disabled={!selectedWs}
            onClick={() => detail && handleInstall(detail.name, detail.display_name || detail.name)}
          >
            安装到当前工作空间
          </Button>,
        ]}
        onCancel={() => setDetail(null)}
      >
        {detail && (
          <>
            <Descriptions size="small" column={2} style={{ marginBottom: 16 }}>
              <Descriptions.Item label="名称">{detail.name}</Descriptions.Item>
              <Descriptions.Item label="版本">{detail.version}</Descriptions.Item>
              <Descriptions.Item label="分类">{detail.category}</Descriptions.Item>
              <Descriptions.Item label="复杂度">
                <Tag color={complexityColor(detail.complexity)}>{complexityLabel(detail.complexity)}</Tag>
              </Descriptions.Item>
            </Descriptions>
            <Paragraph>{detail.description}</Paragraph>
            <Title level={5}>原始定义</Title>
            <pre
              style={{
                background: 'rgba(0,0,0,0.04)',
                padding: 12,
                borderRadius: 8,
                maxHeight: 360,
                overflow: 'auto',
                fontSize: 12,
              }}
            >
              <code>{detail.definition}</code>
            </pre>
          </>
        )}
      </Modal>

      <Modal
        title={`安装「${installModal?.displayName || ''}」到工作空间`}
        open={!!installModal}
        onCancel={() => { setInstallModal(null); setSelectedWs(null); }}
        onOk={doInstall}
        confirmLoading={!!installing}
        okText="安装"
      >
        <Select
          placeholder="选择目标工作空间"
          value={selectedWs}
          onChange={setSelectedWs}
          options={workspaces.map((ws) => ({ label: `${ws.name} (${ws.path})`, value: ws.id }))}
          style={{ width: '100%' }}
        />
      </Modal>
    </div>
  );
}
