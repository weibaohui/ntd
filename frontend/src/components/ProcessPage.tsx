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
} from 'antd';
import { BuildOutlined, ReloadOutlined, EyeOutlined, DownloadOutlined } from '@ant-design/icons';
import bundledApi, { type ProcessTemplate, type ProcessTemplateDetail } from '@/api/bundled';
import { useProjectDirectories } from '@/utils/workspaceDisplay';

const { Title, Text, Paragraph } = Typography;

interface ProcessPageProps {
  workspaceId: number | null;
}

export function ProcessPage({ workspaceId }: ProcessPageProps) {
  const { dirs: workspaces } = useProjectDirectories();
  const [processes, setProcesses] = useState<ProcessTemplate[]>([]);
  const [loading, setLoading] = useState(false);
  const [detail, setDetail] = useState<ProcessTemplateDetail | null>(null);
  const [installing, setInstalling] = useState<string | null>(null);
  const [selectedWs, setSelectedWs] = useState<number | null>(workspaceId);

  // workspaceId 外部变化时同步到本地选择器
  useEffect(() => {
    setSelectedWs(workspaceId);
  }, [workspaceId]);

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

  const handleShowDetail = async (name: string) => {
    try {
      const data = await bundledApi.getProcess(name);
      setDetail(data);
    } catch (e) {
      message.error('加载工艺模板详情失败');
    }
  };

  const handleInstall = async (name: string) => {
    if (!selectedWs) {
      message.warning('请先选择工作空间');
      return;
    }
    setInstalling(name);
    try {
      const result = await bundledApi.installProcess(name, selectedWs);
      message.success(
        `已安装「${result.loop_name}」，生成 ${result.phase_count} 个阶段、${result.step_count} 个环节`
      );
    } catch (e) {
      message.error('安装工艺模板失败');
    } finally {
      setInstalling(null);
    }
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
          <Select
            placeholder="选择安装工作空间"
            value={selectedWs}
            onChange={setSelectedWs}
            options={workspaces.map((ws) => ({ label: `${ws.name} (${ws.path})`, value: ws.id }))}
            style={{ minWidth: 220 }}
          />
          <Button icon={<ReloadOutlined />} onClick={load} loading={loading}>
            刷新
          </Button>
        </div>
      </div>

      {loading && processes.length === 0 ? (
        <div style={{ textAlign: 'center', padding: 64 }}>
          <Spin size="large" />
        </div>
      ) : processes.length === 0 ? (
        <Empty description="暂无工艺模板，请先执行 bundled 同步" />
      ) : (
        <Row gutter={[16, 16]}>
          {processes.map((p) => (
            <Col xs={24} sm={12} lg={8} key={p.id}>
              <Card
                hoverable
                title={p.display_name || p.name}
                extra={<Tag color={complexityColor(p.complexity)}>{complexityLabel(p.complexity)}</Tag>}
                actions={[
                  <Button key="view" type="text" icon={<EyeOutlined />} onClick={() => handleShowDetail(p.name)}>
                    详情
                  </Button>,
                  <Button
                    key="install"
                    type="text"
                    icon={<DownloadOutlined />}
                    loading={installing === p.name}
                    disabled={!selectedWs}
                    onClick={() => handleInstall(p.name)}
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
            onClick={() => detail && handleInstall(detail.name)}
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
    </div>
  );
}
