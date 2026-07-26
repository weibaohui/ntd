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
  Tabs,
  Table,
} from 'antd';
import { BuildOutlined, ReloadOutlined, EyeOutlined, DownloadOutlined, SearchOutlined, ApartmentOutlined, CodeOutlined } from '@ant-design/icons';
import bundledApi, { type ProcessTemplate, type ProcessTemplateDetail, type ProcessLoopItem } from '@/api/bundled';
import { adaptProcessDefinition } from '@/components/process/processFlowAdapter';
import { ProcessFlowGraph } from '@/components/process/ProcessFlowGraph';

const { Title, Text, Paragraph } = Typography;

interface ProcessPageProps {
  workspaceId: number | null;
  /** 安装成功后跳转新环路详情（「工艺 → 环路」实例化关系显性化的关键一跳）。 */
  onOpenLoop?: (loopId: number) => void;
  /**
   * URL 携带的工艺模板唯一名（`/#/processes?name=xxx`）。
   * 用于「环路详情 → 来源工艺」回跳后自动打开该工艺详情，形成溯源闭环。
   */
  processName?: string | null;
}

export function ProcessPage({ workspaceId, onOpenLoop, processName }: ProcessPageProps) {
  const [processes, setProcesses] = useState<ProcessTemplate[]>([]);
  const [loading, setLoading] = useState(false);
  const [detail, setDetail] = useState<ProcessTemplateDetail | null>(null);
  const [installing, setInstalling] = useState<string | null>(null);
  const [searchText, setSearchText] = useState('');
  const [recommended, setRecommended] = useState<string[]>([]);
  const [installModal, setInstallModal] = useState<{ name: string; displayName: string } | null>(null);
  const [instanceLoops, setInstanceLoops] = useState<ProcessLoopItem[]>([]);
  const [instanceLoopsLoading, setInstanceLoopsLoading] = useState(false);

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

  const fetchInstanceLoops = async (templateName: string) => {
    setInstanceLoopsLoading(true);
    try {
      const list = await bundledApi.listProcessLoops(templateName);
      setInstanceLoops(list);
    } catch {
      // 实例环路列表加载失败（如该工艺尚未安装）不弹框，静默空列表。
      setInstanceLoops([]);
    } finally {
      setInstanceLoopsLoading(false);
    }
  };

  // URL 携带 name 参数时自动打开详情：环路详情「来源工艺」回跳的目标落地。
  // cancelled 防御快速切换路由造成的竞态：晚返回的请求发现已卸载/切换就丢弃。
  useEffect(() => {
    if (!processName) return;
    let cancelled = false;
    bundledApi.getProcess(processName)
      .then((data) => { if (!cancelled) setDetail(data); })
      .catch(() => { if (!cancelled) message.error('加载工艺模板详情失败'); });
    return () => { cancelled = true; };
  }, [processName]);

  const handleInstall = (name: string, displayName: string) => {
    setInstallModal({ name, displayName });
  };

  const doInstall = async () => {
    if (!installModal || !workspaceId) { message.warning('请先在左上角选择工作空间'); return; }
    setInstalling(installModal.name);
    try {
      const result = await bundledApi.installProcess(installModal.name, workspaceId);
      message.success(`已安装「${result.loop_name}」，正在打开环路详情…`);
      setInstallModal(null);
      // 安装即实例化：直接跳到新环路详情，让用户立即看到工艺的运行时形态；
      // 未注入导航回调（如旧调用方）时保持原有停留行为，不破坏兼容性。
      if (onOpenLoop) {
        setDetail(null);
        onOpenLoop(result.loop_id);
      }
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
        width={960}
        centered
        footer={[
          <Button key="close" onClick={() => setDetail(null)}>
            关闭
          </Button>,
          <Button
            key="install"
            type="primary"
            icon={<DownloadOutlined />}
            loading={installing === detail?.name}
            disabled={!workspaceId}
            onClick={() => detail && handleInstall(detail.name, detail.display_name || detail.name)}
          >
            安装到当前工作空间
          </Button>,
        ]}
        onCancel={() => setDetail(null)}
      >
        {detail && (
          <Tabs
            defaultActiveKey="flow"
            onChange={(key) => {
              if (key === 'instances') {
                fetchInstanceLoops(detail.name);
              }
            }}
            items={[
              {
                key: 'flow',
                label: <span><ApartmentOutlined /> 流程图</span>,
                children: (() => {
                  const adapted = adaptProcessDefinition(detail.definition);
                  if (!adapted) {
                    return <div style={{ color: '#94a3b8', textAlign: 'center', padding: 60 }}>该工艺定义无法解析，请查看 YAML 源排查语法</div>;
                  }
                  return (
                    <div>
                      <Descriptions size="small" column={2} style={{ marginBottom: 12 }}>
                        <Descriptions.Item label="名称">{detail.name}</Descriptions.Item>
                        <Descriptions.Item label="版本">{detail.version}</Descriptions.Item>
                        <Descriptions.Item label="分类">{detail.category}</Descriptions.Item>
                        <Descriptions.Item label="复杂度">
                          <Tag color={complexityColor(detail.complexity)}>{complexityLabel(detail.complexity)}</Tag>
                        </Descriptions.Item>
                      </Descriptions>
                      <Paragraph type="secondary">{detail.description}</Paragraph>
                      <ProcessFlowGraph
                        links={adapted.links}
                        nodeInputs={adapted.nodeInputs}
                        edgeInputs={adapted.edgeInputs}
                        templateEdges={adapted.templateEdges}
                        phaseGroups={adapted.phaseGroups}
                      />
                    </div>
                  );
                })(),
              },
              {
                key: 'instances',
                label: '实例环路',
                children: instanceLoopsLoading ? (
                  <div style={{ textAlign: 'center', padding: 40 }}><Spin size="small" /></div>
                ) : instanceLoops.length === 0 ? (
                  <Empty description="该工艺尚未安装到任何工作空间" />
                ) : (
                  <Table<ProcessLoopItem>
                    dataSource={instanceLoops}
                    rowKey="id"
                    size="small"
                    pagination={false}
                    columns={[
                      { title: '名称', dataIndex: 'name', key: 'name' },
                      { title: '状态', dataIndex: 'status', key: 'status', render: (s: string) => <Tag color={s === 'active' ? 'green' : 'default'}>{s}</Tag> },
                      { title: '版本', dataIndex: 'process_template_version', key: 'version', render: (v: string | null) => v || '-' },
                      { title: '执行次数', dataIndex: 'execution_count', key: 'count' },
                      {
                        title: '', key: 'action', render: (_: unknown, rec: ProcessLoopItem) => (
                          <Button type="link" size="small" icon={<EyeOutlined />}
                            onClick={() => {
                              setDetail(null);
                              onOpenLoop?.(rec.id);
                            }}
                          >打开</Button>
                        ),
                      },
                    ]}
                  />
                ),
              },
              {
                key: 'yaml',
                label: <span><CodeOutlined /> YAML 源</span>,
                children: (
                  <pre style={{
                    background: 'rgba(0,0,0,0.04)',
                    padding: 12,
                    borderRadius: 8,
                    maxHeight: 400,
                    overflow: 'auto',
                    fontSize: 12,
                    margin: 0,
                  }}>
                    <code>{detail.definition}</code>
                  </pre>
                ),
              },
            ]}
          />
        )}
      </Modal>

      <Modal
        title={`安装工艺模板`}
        open={!!installModal}
        onCancel={() => setInstallModal(null)}
        onOk={doInstall}
        confirmLoading={!!installing}
        okText="安装"
      >
        将「{installModal?.displayName || ''}」安装到当前工作空间？
        {!workspaceId && <div style={{ color: 'red', marginTop: 8 }}>请先在页面左上角选择目标工作空间</div>}
      </Modal>
    </div>
  );
}
